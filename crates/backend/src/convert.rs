//! Conversion utilities between parity trace types and revm-inspector trace types.
//!
//! This module provides conversions from parity's `TransactionTrace` format to
//! revm-inspector's `CallTraceArena` format using the newtype pattern to avoid
//! orphan rule violations.
//!
//! # Features
//!
//! - Converts parity trace structure to revm-inspector's tree-based arena
//! - Handles all call types (Call, DelegateCall, StaticCall, CallCode, AuthCall)
//! - Handles CREATE and CREATE2 operations
//! - Handles selfdestruct operations
//! - Properly orders children based on `trace_address` (not input order)
//! - Populates `ordering` field to track call execution order
//! - Error handling for reverts, out-of-gas, and other failures
//!
//! # Design
//!
//! The conversion logic uses the newtype pattern with wrapper types to implement
//! trait conversions without violating Rust's orphan rules. This design makes it
//! easy to upstream the conversion logic to the respective crates.
//!
//! ## Upstreaming Guide
//!
//! When upstreaming this code to `revm-inspectors` or `alloy`:
//!
//! 1. **Remove the newtype wrappers** (`ParityTraces`, `ParityTrace`)
//! 2. **Convert `TryFrom` implementations** to direct trait implementations:
//!    ```rust
//!    // Current (with newtype):
//!    impl TryFrom<ParityTraces> for CallTraceArena { ... }
//!
//!    // After upstreaming:
//!    impl TryFrom<Vec<TransactionTrace>> for CallTraceArena { ... }
//!    // or
//!    impl CallTraceArena {
//!        pub fn from_parity_traces(traces: Vec<TransactionTrace>) -> Result<Self> { ... }
//!    }
//!    ```
//! 3. **Keep the helper functions** as-is (`call_type_to_call_kind`, `creation_method_to_call_kind`, `error_to_instruction_result`)
//! 4. **Keep all the conversion logic** in the `TryFrom` implementation
//! 5. **Keep the tests** - they are comprehensive and can be adapted with minimal changes
//!
//! ## Example Usage
//!
//! ```ignore
//! use alloy_rpc_types_trace::parity::TransactionTrace;
//! use trace_backend::convert::ParityTraces;
//! use foundry_evm::traces::CallTraceArena;
//!
//! let traces: Vec<TransactionTrace> = // ... from RPC
//! let arena: CallTraceArena = ParityTraces(traces).try_into()?;
//! ```

use alloy_rpc_types_trace::parity::{Action, CallType, CreationMethod, TransactionTrace};
use eyre::Result;
use foundry_evm::{
    revm::interpreter::InstructionResult,
    traces::{CallKind, CallTrace, CallTraceArena, CallTraceNode},
};
use revm_inspectors::tracing::types::TraceMemberOrder;

/// Newtype wrapper around a vector of parity transaction traces.
///
/// This wrapper allows us to implement `From` traits without violating
/// Rust's orphan rules. When upstreaming, this wrapper can be removed
/// and the implementation can be directly on the target type.
#[derive(Debug, Clone)]
pub struct ParityTraces(pub Vec<TransactionTrace>);

impl From<Vec<TransactionTrace>> for ParityTraces {
    fn from(traces: Vec<TransactionTrace>) -> Self {
        Self(traces)
    }
}

impl TryFrom<ParityTraces> for CallTraceArena {
    type Error = eyre::Error;

    fn try_from(parity_traces: ParityTraces) -> Result<Self> {
        let traces = parity_traces.0;

        if traces.is_empty() {
            eyre::bail!("Cannot convert empty trace vector");
        }

        // Verify the first trace is the root
        if !traces[0].trace_address.is_empty() {
            eyre::bail!("First trace must be the root with empty trace_address");
        }

        // Create arena with initial capacity
        // Note: We store (trace_address, node) to enable correct parent lookup and child ordering
        let mut arena_nodes = Vec::with_capacity(traces.len());

        // Process traces in order - they should already be in the correct order from the RPC
        for (idx, trace) in traces.into_iter().enumerate() {
            let depth = trace.trace_address.len();
            let call_trace = ParityTrace(&trace).try_into()?;

            // Determine parent index
            let parent = if depth == 0 {
                None
            } else {
                // Parent is found by removing the last element from trace_address
                // and finding the node with that trace_address
                let parent_trace_address = &trace.trace_address[..depth - 1];

                // Find the parent node
                let parent_idx = arena_nodes
                    .iter()
                    .position(|(addr, _): &(Vec<usize>, CallTraceNode)| {
                        addr == parent_trace_address
                    })
                    .ok_or_else(|| {
                        eyre::eyre!(
                            "Parent trace not found for trace_address {:?}",
                            trace.trace_address
                        )
                    })?;

                Some(parent_idx)
            };

            // Create the node
            let node = CallTraceNode {
                parent,
                children: Vec::new(),
                idx,
                trace: call_trace,
                logs: Vec::new(),
                ordering: Vec::new(),
            };

            // Store with trace_address for lookup
            arena_nodes.push((trace.trace_address.clone(), node));
        }

        // Build children relationships and extract final nodes
        let mut final_nodes: Vec<CallTraceNode> =
            arena_nodes.iter().map(|(_, node)| node.clone()).collect();

        // Build children relationships with proper ordering
        //
        // In parity traces, the last element of trace_address indicates the child's
        // position among its siblings. For example:
        // - trace_address [0] = first child of root (position 0)
        // - trace_address [1] = second child of root (position 1)
        // - trace_address [0, 0] = first child of first child (position 0)
        // - trace_address [0, 1] = second child of first child (position 1)
        //
        // We must respect this ordering to maintain correct execution semantics,
        // regardless of the order traces arrive in the input vector.
        let mut children_with_positions: Vec<Vec<(usize, usize)>> =
            vec![Vec::new(); final_nodes.len()];

        for idx in 0..final_nodes.len() {
            if let Some(parent_idx) = final_nodes[idx].parent {
                // Get the child position from the last element of trace_address
                // This represents the child index among siblings (0, 1, 2, ...)
                let child_position = arena_nodes[idx].0.last().copied().unwrap_or(0);
                children_with_positions[parent_idx].push((child_position, idx));
            }
        }

        // Sort children by their trace_address position and assign to nodes
        // This ensures children are ordered by execution order (0, 1, 2, ...)
        // even if the input traces were provided out of order
        for (node_idx, children) in children_with_positions.iter_mut().enumerate() {
            children.sort_by_key(|(pos, _)| *pos);
            final_nodes[node_idx].children = children.iter().map(|(_, idx)| *idx).collect();

            // Populate ordering field with call indices
            // Note: Parity traces don't include log information, so we only track calls
            // The index in ordering corresponds to the position in the children array
            final_nodes[node_idx].ordering = (0..children.len())
                .map(|i| TraceMemberOrder::Call(i))
                .collect();
        }

        // Create the arena using Default and then replace the nodes
        let mut arena = CallTraceArena::default();
        *arena.nodes_mut() = final_nodes;

        Ok(arena)
    }
}

/// Newtype wrapper around a reference to a parity transaction trace.
///
/// This wrapper allows us to implement `From` traits without violating
/// Rust's orphan rules.
#[derive(Debug, Clone, Copy)]
struct ParityTrace<'a>(&'a TransactionTrace);

impl<'a> TryFrom<ParityTrace<'a>> for CallTrace {
    type Error = eyre::Error;

    fn try_from(parity_trace: ParityTrace<'a>) -> Result<Self> {
        let trace = parity_trace.0;
        let depth = trace.trace_address.len();
        let success = trace.error.is_none();

        // Extract information from the action
        let (caller, address, kind, value, data, gas_limit) = match &trace.action {
            Action::Call(call) => (
                call.from,
                call.to,
                call_type_to_call_kind(call.call_type),
                call.value,
                call.input.clone(),
                call.gas,
            ),
            Action::Create(create) => (
                create.from,
                // For creates, the address is in the result
                trace
                    .result
                    .as_ref()
                    .and_then(|r| r.as_create())
                    .map(|c| c.address)
                    .unwrap_or_default(),
                creation_method_to_call_kind(create.creation_method),
                create.value,
                create.init.clone(),
                create.gas,
            ),
            Action::Selfdestruct(selfdestruct) => {
                // Selfdestructs are special - they're not really calls but we need to represent them
                return Ok(CallTrace {
                    depth,
                    success,
                    caller: selfdestruct.address,
                    address: selfdestruct.refund_address,
                    maybe_precompile: Some(false),
                    selfdestruct_address: Some(selfdestruct.address),
                    selfdestruct_refund_target: Some(selfdestruct.refund_address),
                    selfdestruct_transferred_value: Some(selfdestruct.balance),
                    kind: CallKind::Call,
                    value: selfdestruct.balance,
                    data: Default::default(),
                    output: Default::default(),
                    gas_used: 0,
                    gas_limit: 0,
                    status: Some(InstructionResult::SelfDestruct),
                    steps: Vec::new(),
                    decoded: None,
                });
            }
            Action::Reward(_) => {
                // Rewards are not actual traces we can convert
                eyre::bail!("Cannot convert Reward action to CallTrace");
            }
        };

        // Extract output and gas_used from result
        let (output, gas_used) = if let Some(result) = &trace.result {
            (result.output().clone(), result.gas_used())
        } else {
            (Default::default(), 0)
        };

        // Determine the status based on error
        let status = if let Some(ref error_msg) = trace.error {
            Some(error_to_instruction_result(error_msg))
        } else {
            Some(InstructionResult::Return)
        };

        Ok(CallTrace {
            depth,
            success,
            caller,
            address,
            maybe_precompile: None,
            selfdestruct_address: None,
            selfdestruct_refund_target: None,
            selfdestruct_transferred_value: None,
            kind,
            value,
            data,
            output,
            gas_used,
            gas_limit,
            status,
            steps: Vec::new(),
            decoded: None,
        })
    }
}

/// Converts a parity CallType to a revm-inspector CallKind.
fn call_type_to_call_kind(call_type: CallType) -> CallKind {
    match call_type {
        CallType::None => CallKind::Call,
        CallType::Call => CallKind::Call,
        CallType::CallCode => CallKind::CallCode,
        CallType::DelegateCall => CallKind::DelegateCall,
        CallType::StaticCall => CallKind::StaticCall,
        CallType::AuthCall => CallKind::AuthCall,
    }
}

/// Converts a parity CreationMethod to a revm-inspector CallKind.
fn creation_method_to_call_kind(creation_method: CreationMethod) -> CallKind {
    match creation_method {
        CreationMethod::None => CallKind::Create,
        CreationMethod::Create => CallKind::Create,
        CreationMethod::Create2 => CallKind::Create2,
        CreationMethod::EofCreate => CallKind::Create,
    }
}

/// Converts a parity error string to a revm InstructionResult.
fn error_to_instruction_result(error_msg: &str) -> InstructionResult {
    if error_msg.contains("Revert") || error_msg.contains("revert") {
        InstructionResult::Revert
    } else if error_msg.contains("OutOfGas") {
        InstructionResult::OutOfGas
    } else {
        // Generic error - default to Revert
        InstructionResult::Revert
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{Address, Bytes, U256};
    use alloy_rpc_types_trace::parity::{
        CallAction, CallOutput, CallType, CreateAction, CreateOutput, CreationMethod, RewardAction,
        RewardType, SelfdestructAction, TraceOutput,
    };

    #[test]
    fn test_simple_call_trace() {
        // Create a simple call trace
        let trace = TransactionTrace {
            action: Action::Call(CallAction {
                from: Address::from([1u8; 20]),
                to: Address::from([2u8; 20]),
                value: U256::from(100),
                gas: 21000,
                input: Bytes::from(vec![0x12, 0x34]),
                call_type: CallType::Call,
            }),
            result: Some(TraceOutput::Call(CallOutput {
                gas_used: 20000,
                output: Bytes::from(vec![0xab, 0xcd]),
            })),
            error: None,
            subtraces: 0,
            trace_address: vec![],
        };

        let arena: CallTraceArena = ParityTraces(vec![trace])
            .try_into()
            .expect("Failed to convert trace");

        assert_eq!(arena.nodes().len(), 1);
        let node = &arena.nodes()[0];
        assert_eq!(node.trace.depth, 0);
        assert_eq!(node.trace.caller, Address::from([1u8; 20]));
        assert_eq!(node.trace.address, Address::from([2u8; 20]));
        assert_eq!(node.trace.value, U256::from(100));
        assert_eq!(node.trace.gas_limit, 21000);
        assert_eq!(node.trace.gas_used, 20000);
        assert_eq!(node.trace.data, Bytes::from(vec![0x12, 0x34]));
        assert_eq!(node.trace.output, Bytes::from(vec![0xab, 0xcd]));
        assert!(node.trace.success);
        assert_eq!(node.trace.kind, CallKind::Call);
        assert_eq!(node.parent, None);
        assert_eq!(node.children.len(), 0);
    }

    #[test]
    fn test_create_trace() {
        let created_address = Address::from([3u8; 20]);
        let trace = TransactionTrace {
            action: Action::Create(CreateAction {
                from: Address::from([1u8; 20]),
                value: U256::from(0),
                gas: 100000,
                init: Bytes::from(vec![0x60, 0x80]),
                creation_method: CreationMethod::Create,
            }),
            result: Some(TraceOutput::Create(CreateOutput {
                address: created_address,
                code: Bytes::from(vec![0x60, 0x60]),
                gas_used: 50000,
            })),
            error: None,
            subtraces: 0,
            trace_address: vec![],
        };

        let arena: CallTraceArena = ParityTraces(vec![trace])
            .try_into()
            .expect("Failed to convert trace");

        assert_eq!(arena.nodes().len(), 1);
        let node = &arena.nodes()[0];
        assert_eq!(node.trace.depth, 0);
        assert_eq!(node.trace.caller, Address::from([1u8; 20]));
        assert_eq!(node.trace.address, created_address);
        assert_eq!(node.trace.kind, CallKind::Create);
        assert_eq!(node.trace.data, Bytes::from(vec![0x60, 0x80]));
        assert_eq!(node.trace.output, Bytes::from(vec![0x60, 0x60]));
        assert_eq!(node.trace.gas_used, 50000);
        assert!(node.trace.success);
    }

    #[test]
    fn test_create2_trace() {
        let created_address = Address::from([3u8; 20]);
        let trace = TransactionTrace {
            action: Action::Create(CreateAction {
                from: Address::from([1u8; 20]),
                value: U256::from(0),
                gas: 100000,
                init: Bytes::from(vec![0x60, 0x80]),
                creation_method: CreationMethod::Create2,
            }),
            result: Some(TraceOutput::Create(CreateOutput {
                address: created_address,
                code: Bytes::from(vec![0x60, 0x60]),
                gas_used: 50000,
            })),
            error: None,
            subtraces: 0,
            trace_address: vec![],
        };

        let arena: CallTraceArena = ParityTraces(vec![trace])
            .try_into()
            .expect("Failed to convert trace");

        assert_eq!(arena.nodes().len(), 1);
        let node = &arena.nodes()[0];
        assert_eq!(node.trace.kind, CallKind::Create2);
    }

    #[test]
    fn test_nested_calls() {
        // Root call
        let root_trace = TransactionTrace {
            action: Action::Call(CallAction {
                from: Address::from([1u8; 20]),
                to: Address::from([2u8; 20]),
                value: U256::from(100),
                gas: 100000,
                input: Bytes::default(),
                call_type: CallType::Call,
            }),
            result: Some(TraceOutput::Call(CallOutput {
                gas_used: 80000,
                output: Bytes::default(),
            })),
            error: None,
            subtraces: 2,
            trace_address: vec![],
        };

        // First child call
        let child1_trace = TransactionTrace {
            action: Action::Call(CallAction {
                from: Address::from([2u8; 20]),
                to: Address::from([3u8; 20]),
                value: U256::from(50),
                gas: 50000,
                input: Bytes::default(),
                call_type: CallType::Call,
            }),
            result: Some(TraceOutput::Call(CallOutput {
                gas_used: 30000,
                output: Bytes::default(),
            })),
            error: None,
            subtraces: 0,
            trace_address: vec![0],
        };

        // Second child call
        let child2_trace = TransactionTrace {
            action: Action::Call(CallAction {
                from: Address::from([2u8; 20]),
                to: Address::from([4u8; 20]),
                value: U256::from(25),
                gas: 40000,
                input: Bytes::default(),
                call_type: CallType::DelegateCall,
            }),
            result: Some(TraceOutput::Call(CallOutput {
                gas_used: 20000,
                output: Bytes::default(),
            })),
            error: None,
            subtraces: 0,
            trace_address: vec![1],
        };

        let arena: CallTraceArena = ParityTraces(vec![root_trace, child1_trace, child2_trace])
            .try_into()
            .expect("Failed to convert traces");

        assert_eq!(arena.nodes().len(), 3);

        // Check root node
        let root = &arena.nodes()[0];
        assert_eq!(root.trace.depth, 0);
        assert_eq!(root.parent, None);
        assert_eq!(root.children.len(), 2);
        assert_eq!(root.children, vec![1, 2]);
        // Verify ordering
        assert_eq!(root.ordering.len(), 2);
        assert_eq!(root.ordering[0], TraceMemberOrder::Call(0));
        assert_eq!(root.ordering[1], TraceMemberOrder::Call(1));

        // Check first child
        let child1 = &arena.nodes()[1];
        assert_eq!(child1.trace.depth, 1);
        assert_eq!(child1.parent, Some(0));
        assert_eq!(child1.children.len(), 0);
        assert_eq!(child1.trace.address, Address::from([3u8; 20]));
        assert_eq!(child1.ordering.len(), 0); // No children, no ordering

        // Check second child
        let child2 = &arena.nodes()[2];
        assert_eq!(child2.trace.depth, 1);
        assert_eq!(child2.parent, Some(0));
        assert_eq!(child2.children.len(), 0);
        assert_eq!(child2.trace.kind, CallKind::DelegateCall);
        assert_eq!(child2.ordering.len(), 0); // No children, no ordering
    }

    #[test]
    fn test_deeply_nested_calls() {
        // Root -> Child -> Grandchild
        let root = TransactionTrace {
            action: Action::Call(CallAction {
                from: Address::from([1u8; 20]),
                to: Address::from([2u8; 20]),
                value: U256::from(100),
                gas: 100000,
                input: Bytes::default(),
                call_type: CallType::Call,
            }),
            result: Some(TraceOutput::Call(CallOutput {
                gas_used: 90000,
                output: Bytes::default(),
            })),
            error: None,
            subtraces: 1,
            trace_address: vec![],
        };

        let child = TransactionTrace {
            action: Action::Call(CallAction {
                from: Address::from([2u8; 20]),
                to: Address::from([3u8; 20]),
                value: U256::from(50),
                gas: 50000,
                input: Bytes::default(),
                call_type: CallType::Call,
            }),
            result: Some(TraceOutput::Call(CallOutput {
                gas_used: 40000,
                output: Bytes::default(),
            })),
            error: None,
            subtraces: 1,
            trace_address: vec![0],
        };

        let grandchild = TransactionTrace {
            action: Action::Call(CallAction {
                from: Address::from([3u8; 20]),
                to: Address::from([4u8; 20]),
                value: U256::from(25),
                gas: 25000,
                input: Bytes::default(),
                call_type: CallType::StaticCall,
            }),
            result: Some(TraceOutput::Call(CallOutput {
                gas_used: 20000,
                output: Bytes::default(),
            })),
            error: None,
            subtraces: 0,
            trace_address: vec![0, 0],
        };

        let arena: CallTraceArena = ParityTraces(vec![root, child, grandchild])
            .try_into()
            .expect("Failed to convert traces");

        assert_eq!(arena.nodes().len(), 3);

        let root_node = &arena.nodes()[0];
        assert_eq!(root_node.trace.depth, 0);
        assert_eq!(root_node.children, vec![1]);
        assert_eq!(root_node.ordering, vec![TraceMemberOrder::Call(0)]);

        let child_node = &arena.nodes()[1];
        assert_eq!(child_node.trace.depth, 1);
        assert_eq!(child_node.parent, Some(0));
        assert_eq!(child_node.children, vec![2]);
        assert_eq!(child_node.ordering, vec![TraceMemberOrder::Call(0)]);

        let grandchild_node = &arena.nodes()[2];
        assert_eq!(grandchild_node.trace.depth, 2);
        assert_eq!(grandchild_node.parent, Some(1));
        assert_eq!(grandchild_node.children.len(), 0);
        assert_eq!(grandchild_node.trace.kind, CallKind::StaticCall);
        assert_eq!(grandchild_node.ordering.len(), 0);
    }

    #[test]
    fn test_error_trace() {
        let trace = TransactionTrace {
            action: Action::Call(CallAction {
                from: Address::from([1u8; 20]),
                to: Address::from([2u8; 20]),
                value: U256::from(100),
                gas: 21000,
                input: Bytes::default(),
                call_type: CallType::Call,
            }),
            result: Some(TraceOutput::Call(CallOutput {
                gas_used: 21000,
                output: Bytes::default(),
            })),
            error: Some("Reverted".to_string()),
            subtraces: 0,
            trace_address: vec![],
        };

        let arena: CallTraceArena = ParityTraces(vec![trace])
            .try_into()
            .expect("Failed to convert trace");

        assert_eq!(arena.nodes().len(), 1);
        let node = &arena.nodes()[0];
        assert!(!node.trace.success);
        assert_eq!(node.trace.status, Some(InstructionResult::Revert));
    }

    #[test]
    fn test_out_of_gas_error() {
        let trace = TransactionTrace {
            action: Action::Call(CallAction {
                from: Address::from([1u8; 20]),
                to: Address::from([2u8; 20]),
                value: U256::from(100),
                gas: 21000,
                input: Bytes::default(),
                call_type: CallType::Call,
            }),
            result: None,
            error: Some("OutOfGas".to_string()),
            subtraces: 0,
            trace_address: vec![],
        };

        let arena: CallTraceArena = ParityTraces(vec![trace])
            .try_into()
            .expect("Failed to convert trace");

        assert_eq!(arena.nodes().len(), 1);
        let node = &arena.nodes()[0];
        assert!(!node.trace.success);
        assert_eq!(node.trace.status, Some(InstructionResult::OutOfGas));
        assert_eq!(node.trace.gas_used, 0);
    }

    #[test]
    fn test_selfdestruct_trace() {
        let trace = TransactionTrace {
            action: Action::Selfdestruct(SelfdestructAction {
                address: Address::from([1u8; 20]),
                refund_address: Address::from([2u8; 20]),
                balance: U256::from(1000),
            }),
            result: None,
            error: None,
            subtraces: 0,
            trace_address: vec![],
        };

        let arena: CallTraceArena = ParityTraces(vec![trace])
            .try_into()
            .expect("Failed to convert trace");

        assert_eq!(arena.nodes().len(), 1);
        let node = &arena.nodes()[0];
        assert_eq!(node.trace.caller, Address::from([1u8; 20]));
        assert_eq!(node.trace.address, Address::from([2u8; 20]));
        assert_eq!(
            node.trace.selfdestruct_address,
            Some(Address::from([1u8; 20]))
        );
        assert_eq!(
            node.trace.selfdestruct_refund_target,
            Some(Address::from([2u8; 20]))
        );
        assert_eq!(
            node.trace.selfdestruct_transferred_value,
            Some(U256::from(1000))
        );
        assert_eq!(node.trace.status, Some(InstructionResult::SelfDestruct));
    }

    #[test]
    fn test_staticcall_trace() {
        let trace = TransactionTrace {
            action: Action::Call(CallAction {
                from: Address::from([1u8; 20]),
                to: Address::from([2u8; 20]),
                value: U256::ZERO,
                gas: 50000,
                input: Bytes::default(),
                call_type: CallType::StaticCall,
            }),
            result: Some(TraceOutput::Call(CallOutput {
                gas_used: 30000,
                output: Bytes::default(),
            })),
            error: None,
            subtraces: 0,
            trace_address: vec![],
        };

        let arena: CallTraceArena = ParityTraces(vec![trace])
            .try_into()
            .expect("Failed to convert trace");

        assert_eq!(arena.nodes().len(), 1);
        let node = &arena.nodes()[0];
        assert_eq!(node.trace.kind, CallKind::StaticCall);
    }

    #[test]
    fn test_callcode_trace() {
        let trace = TransactionTrace {
            action: Action::Call(CallAction {
                from: Address::from([1u8; 20]),
                to: Address::from([2u8; 20]),
                value: U256::from(100),
                gas: 50000,
                input: Bytes::default(),
                call_type: CallType::CallCode,
            }),
            result: Some(TraceOutput::Call(CallOutput {
                gas_used: 30000,
                output: Bytes::default(),
            })),
            error: None,
            subtraces: 0,
            trace_address: vec![],
        };

        let arena: CallTraceArena = ParityTraces(vec![trace])
            .try_into()
            .expect("Failed to convert trace");

        assert_eq!(arena.nodes().len(), 1);
        let node = &arena.nodes()[0];
        assert_eq!(node.trace.kind, CallKind::CallCode);
    }

    #[test]
    fn test_empty_trace_vector() {
        let result: Result<CallTraceArena> = ParityTraces(vec![]).try_into();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_invalid_root_trace() {
        let trace = TransactionTrace {
            action: Action::Call(CallAction {
                from: Address::from([1u8; 20]),
                to: Address::from([2u8; 20]),
                value: U256::from(100),
                gas: 21000,
                input: Bytes::default(),
                call_type: CallType::Call,
            }),
            result: Some(TraceOutput::Call(CallOutput {
                gas_used: 20000,
                output: Bytes::default(),
            })),
            error: None,
            subtraces: 0,
            trace_address: vec![0], // Should be empty for root
        };

        let result: Result<CallTraceArena> = ParityTraces(vec![trace]).try_into();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("root"));
    }

    #[test]
    fn test_reward_action_fails() {
        let trace = TransactionTrace {
            action: Action::Reward(RewardAction {
                author: Address::from([1u8; 20]),
                value: U256::from(1000),
                reward_type: RewardType::Block,
            }),
            result: None,
            error: None,
            subtraces: 0,
            trace_address: vec![],
        };

        let result: Result<CallTraceArena> = ParityTraces(vec![trace]).try_into();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Reward"));
    }

    #[test]
    fn test_out_of_order_children() {
        // Test that children are correctly ordered even if traces arrive out of order
        // Root with children [0], [1], [2] but provided in order: [], [0], [2], [1]
        let traces = vec![
            // Root
            TransactionTrace {
                action: Action::Call(CallAction {
                    from: Address::from([1u8; 20]),
                    to: Address::from([2u8; 20]),
                    value: U256::from(100),
                    gas: 100000,
                    input: Bytes::default(),
                    call_type: CallType::Call,
                }),
                result: Some(TraceOutput::Call(CallOutput {
                    gas_used: 90000,
                    output: Bytes::default(),
                })),
                error: None,
                subtraces: 3,
                trace_address: vec![],
            },
            // First child [0] - arrives first
            TransactionTrace {
                action: Action::Call(CallAction {
                    from: Address::from([2u8; 20]),
                    to: Address::from([3u8; 20]),
                    value: U256::from(50),
                    gas: 50000,
                    input: Bytes::default(),
                    call_type: CallType::Call,
                }),
                result: Some(TraceOutput::Call(CallOutput {
                    gas_used: 40000,
                    output: Bytes::default(),
                })),
                error: None,
                subtraces: 0,
                trace_address: vec![0],
            },
            // Third child [2] - arrives before second child!
            TransactionTrace {
                action: Action::Call(CallAction {
                    from: Address::from([2u8; 20]),
                    to: Address::from([5u8; 20]),
                    value: U256::from(30),
                    gas: 30000,
                    input: Bytes::default(),
                    call_type: CallType::Call,
                }),
                result: Some(TraceOutput::Call(CallOutput {
                    gas_used: 25000,
                    output: Bytes::default(),
                })),
                error: None,
                subtraces: 0,
                trace_address: vec![2],
            },
            // Second child [1] - arrives last
            TransactionTrace {
                action: Action::Call(CallAction {
                    from: Address::from([2u8; 20]),
                    to: Address::from([4u8; 20]),
                    value: U256::from(25),
                    gas: 25000,
                    input: Bytes::default(),
                    call_type: CallType::Call,
                }),
                result: Some(TraceOutput::Call(CallOutput {
                    gas_used: 20000,
                    output: Bytes::default(),
                })),
                error: None,
                subtraces: 0,
                trace_address: vec![1],
            },
        ];

        let arena: CallTraceArena = ParityTraces(traces)
            .try_into()
            .expect("Failed to convert traces");

        assert_eq!(arena.nodes().len(), 4);

        // Root should have children in correct order: [1, 3, 2] based on trace_address [0], [1], [2]
        // NOT in arrival order [1, 2, 3]
        let root = &arena.nodes()[0];
        assert_eq!(root.children.len(), 3);

        // Verify children are sorted by trace_address index
        // Child with trace_address [0] should be first
        assert_eq!(
            arena.nodes()[root.children[0]].trace.address,
            Address::from([3u8; 20])
        );
        // Child with trace_address [1] should be second
        assert_eq!(
            arena.nodes()[root.children[1]].trace.address,
            Address::from([4u8; 20])
        );
        // Child with trace_address [2] should be third
        assert_eq!(
            arena.nodes()[root.children[2]].trace.address,
            Address::from([5u8; 20])
        );

        // Verify ordering field is populated correctly
        assert_eq!(root.ordering.len(), 3);
        assert_eq!(root.ordering[0], TraceMemberOrder::Call(0));
        assert_eq!(root.ordering[1], TraceMemberOrder::Call(1));
        assert_eq!(root.ordering[2], TraceMemberOrder::Call(2));
    }

    #[test]
    fn test_complex_tree_structure() {
        // Root with 3 children, first child has 2 children
        let traces = vec![
            // Root [0]
            TransactionTrace {
                action: Action::Call(CallAction {
                    from: Address::from([1u8; 20]),
                    to: Address::from([2u8; 20]),
                    value: U256::from(100),
                    gas: 100000,
                    input: Bytes::default(),
                    call_type: CallType::Call,
                }),
                result: Some(TraceOutput::Call(CallOutput {
                    gas_used: 90000,
                    output: Bytes::default(),
                })),
                error: None,
                subtraces: 3,
                trace_address: vec![],
            },
            // First child [0, 0]
            TransactionTrace {
                action: Action::Call(CallAction {
                    from: Address::from([2u8; 20]),
                    to: Address::from([3u8; 20]),
                    value: U256::from(50),
                    gas: 50000,
                    input: Bytes::default(),
                    call_type: CallType::Call,
                }),
                result: Some(TraceOutput::Call(CallOutput {
                    gas_used: 40000,
                    output: Bytes::default(),
                })),
                error: None,
                subtraces: 2,
                trace_address: vec![0],
            },
            // First grandchild [0, 0, 0]
            TransactionTrace {
                action: Action::Call(CallAction {
                    from: Address::from([3u8; 20]),
                    to: Address::from([5u8; 20]),
                    value: U256::from(10),
                    gas: 10000,
                    input: Bytes::default(),
                    call_type: CallType::Call,
                }),
                result: Some(TraceOutput::Call(CallOutput {
                    gas_used: 5000,
                    output: Bytes::default(),
                })),
                error: None,
                subtraces: 0,
                trace_address: vec![0, 0],
            },
            // Second grandchild [0, 0, 1]
            TransactionTrace {
                action: Action::Call(CallAction {
                    from: Address::from([3u8; 20]),
                    to: Address::from([6u8; 20]),
                    value: U256::from(15),
                    gas: 15000,
                    input: Bytes::default(),
                    call_type: CallType::Call,
                }),
                result: Some(TraceOutput::Call(CallOutput {
                    gas_used: 10000,
                    output: Bytes::default(),
                })),
                error: None,
                subtraces: 0,
                trace_address: vec![0, 1],
            },
            // Second child [0, 1]
            TransactionTrace {
                action: Action::Call(CallAction {
                    from: Address::from([2u8; 20]),
                    to: Address::from([4u8; 20]),
                    value: U256::from(25),
                    gas: 25000,
                    input: Bytes::default(),
                    call_type: CallType::Call,
                }),
                result: Some(TraceOutput::Call(CallOutput {
                    gas_used: 20000,
                    output: Bytes::default(),
                })),
                error: None,
                subtraces: 0,
                trace_address: vec![1],
            },
            // Third child [0, 2]
            TransactionTrace {
                action: Action::Call(CallAction {
                    from: Address::from([2u8; 20]),
                    to: Address::from([7u8; 20]),
                    value: U256::from(30),
                    gas: 30000,
                    input: Bytes::default(),
                    call_type: CallType::Call,
                }),
                result: Some(TraceOutput::Call(CallOutput {
                    gas_used: 25000,
                    output: Bytes::default(),
                })),
                error: None,
                subtraces: 0,
                trace_address: vec![2],
            },
        ];

        let arena: CallTraceArena = ParityTraces(traces)
            .try_into()
            .expect("Failed to convert traces");

        assert_eq!(arena.nodes().len(), 6);

        // Root has 3 children
        assert_eq!(arena.nodes()[0].children.len(), 3);
        assert_eq!(arena.nodes()[0].children, vec![1, 4, 5]);
        assert_eq!(
            arena.nodes()[0].ordering,
            vec![
                TraceMemberOrder::Call(0),
                TraceMemberOrder::Call(1),
                TraceMemberOrder::Call(2)
            ]
        );

        // First child has 2 children
        assert_eq!(arena.nodes()[1].children.len(), 2);
        assert_eq!(arena.nodes()[1].children, vec![2, 3]);
        assert_eq!(arena.nodes()[1].parent, Some(0));
        assert_eq!(
            arena.nodes()[1].ordering,
            vec![TraceMemberOrder::Call(0), TraceMemberOrder::Call(1)]
        );

        // First grandchild
        assert_eq!(arena.nodes()[2].parent, Some(1));
        assert_eq!(arena.nodes()[2].children.len(), 0);
        assert_eq!(arena.nodes()[2].ordering.len(), 0);

        // Second grandchild
        assert_eq!(arena.nodes()[3].parent, Some(1));
        assert_eq!(arena.nodes()[3].children.len(), 0);
        assert_eq!(arena.nodes()[3].ordering.len(), 0);

        // Second and third children of root
        assert_eq!(arena.nodes()[4].parent, Some(0));
        assert_eq!(arena.nodes()[4].ordering.len(), 0);
        assert_eq!(arena.nodes()[5].parent, Some(0));
        assert_eq!(arena.nodes()[5].ordering.len(), 0);
    }
}
