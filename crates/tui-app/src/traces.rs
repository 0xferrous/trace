use ratatui::text::Text;
use revm_inspectors::tracing::{
    CallTraceArena,
    types::{CallTraceNode, TraceMemberOrder},
};

use crate::trace_writer::{SelectionStyle, TraceTextWriter};

fn is_returning_node(node: &CallTraceNode) -> bool {
    let val = node.trace.status.unwrap_or_default() as u8;
    // RETURN or REVERT
    val == 2 || val == 16
}

/// Indicates where we are in the trace
#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub enum ActiveItem {
    #[default]
    /// This means we are at the call trace header
    CallHeader,
    /// This means we are the log at the specific index in node.logs[idx]
    Item(usize),
    /// This means we are at the call footer, only for returning/reverting calls
    ReturningValue,
}

impl ActiveItem {
    pub fn is_call_header(&self) -> bool {
        matches!(self, ActiveItem::CallHeader)
    }

    pub fn is_item(&self) -> bool {
        matches!(self, ActiveItem::Item(_))
    }

    pub fn is_returning_value(&self) -> bool {
        matches!(self, ActiveItem::ReturningValue)
    }
}

#[derive(Debug, Default)]
pub struct ActiveCall {
    pub idx: usize,
    pub active_item: ActiveItem,
}

impl ActiveCall {
    /// Jump to a specific call node, positioning at its header
    fn jump_to_call(&mut self, idx: usize) {
        self.idx = idx;
        self.active_item = ActiveItem::CallHeader;
    }

    /// Jump to a specific item within a call node
    fn jump_to_call_item(&mut self, idx: usize, item_idx: usize) {
        self.idx = idx;
        self.active_item = ActiveItem::Item(item_idx);
    }

    /// If the current item is a call, follow it into that call's header.
    /// Returns true if we successfully followed into a child call.
    fn follow(&mut self, nodes: &[CallTraceNode]) -> bool {
        let curr_node = &nodes[self.idx];
        if let ActiveItem::Item(idx) = self.active_item {
            let item = &curr_node.ordering[idx];
            if let TraceMemberOrder::Call(idx) = item {
                log::trace!("follow: jump_to_call: {idx}");
                self.jump_to_call(curr_node.children[*idx]);
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Navigate from the current call to its position within the parent's ordering.
    /// This effectively "zooms out" one level in the call tree.
    /// Returns false if we're already at the root call.
    fn jump_to_parent_call_step(&mut self, nodes: &[CallTraceNode]) -> bool {
        log::trace!("jump_to_parent_call_step: {:?}", self.active_item);
        if let Some(parent) = nodes[self.idx].parent {
            let parent = &nodes[parent];
            // Find which child index we are in the parent's children list
            let child_idx = parent
                .children
                .iter()
                .position(|&idx| idx == self.idx)
                .expect("curr_idx not found in parent_node.children");
            // Find where that child call appears in the parent's ordering
            let item_idx = parent
                .ordering
                .iter()
                .position(|item| *item == TraceMemberOrder::Call(child_idx))
                .expect("curr_child not found in ordering");
            self.jump_to_call_item(parent.idx, item_idx);
            true
        } else {
            false
        }
    }

    /// Move to the next item in the trace sequence.
    /// If at CallHeader, moves up to parent and tries again.
    /// If at Item, moves to next item or to ReturningValue.
    /// If at ReturningValue, cannot move forward (returns false).
    fn next_step(&mut self, nodes: &[CallTraceNode]) -> bool {
        match self.active_item {
            ActiveItem::CallHeader => self.jump_to_parent_call_step(nodes) && self.next_step(nodes),
            ActiveItem::Item(idx) => {
                let curr_node = &nodes[self.idx];
                let items_len = curr_node.ordering.len();
                let next_idx = idx + 1;
                if next_idx < items_len {
                    self.active_item = ActiveItem::Item(next_idx);
                    self.follow(nodes);
                    true
                } else if is_returning_node(curr_node) {
                    self.active_item = ActiveItem::ReturningValue;
                    true
                } else {
                    false
                }
            }
            ActiveItem::ReturningValue => false,
        }
    }

    /// Move to the previous item in the trace sequence.
    /// If at CallHeader, moves up to parent and tries again.
    /// If at Item(0), cannot move backward (returns false).
    /// If at ReturningValue, moves to last item in the call.
    fn prev_step(&mut self, nodes: &[CallTraceNode]) -> bool {
        log::trace!("prev_step: {:?}", self.active_item);
        match self.active_item {
            ActiveItem::CallHeader => self.jump_to_parent_call_step(nodes) && self.prev_step(nodes),
            ActiveItem::Item(idx) => {
                if idx == 0 {
                    false
                } else {
                    self.active_item = ActiveItem::Item(idx - 1);
                    self.follow(nodes);
                    true
                }
            }
            ActiveItem::ReturningValue => {
                let curr_node = &nodes[self.idx];
                let items_len = curr_node.ordering.len();
                if items_len == 0 {
                    return false;
                }
                self.active_item = ActiveItem::Item(items_len - 1);
                self.follow(nodes);
                true
            }
        }
    }

    /// Step into the current call header to view its first item.
    /// Only works when positioned at CallHeader.
    fn step_into(&mut self, nodes: &[CallTraceNode]) -> bool {
        if matches!(self.active_item, ActiveItem::CallHeader) {
            let curr_node = &nodes[self.idx];
            if !curr_node.ordering.is_empty() {
                self.jump_to_call_item(curr_node.idx, 0);
                self.follow(nodes);
                true
            } else if is_returning_node(curr_node) {
                self.active_item = ActiveItem::ReturningValue;
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Navigate to the deepest last item within the current call.
    /// Recursively follows the last call child until reaching a log or return value.
    /// Only works when positioned at CallHeader.
    fn lastest_item(&mut self, nodes: &[CallTraceNode]) -> bool {
        log::trace!("lastest_item: {:?} {}", self.active_item, self.idx);
        if matches!(self.active_item, ActiveItem::CallHeader) {
            let curr_node = &nodes[self.idx];
            if is_returning_node(curr_node) {
                self.active_item = ActiveItem::ReturningValue;
                true
            } else if let Some(last_item) = curr_node.ordering.last() {
                match last_item {
                    TraceMemberOrder::Call(idx) => {
                        // Recursively find the last item in the child call
                        self.jump_to_call(curr_node.children[*idx]);
                        self.lastest_item(nodes)
                    }
                    TraceMemberOrder::Log(_) => {
                        self.active_item = ActiveItem::Item(curr_node.ordering.len() - 1);
                        log::trace!("lastest_item: returning true: {:?}", self.active_item);
                        true
                    }
                    _ => false,
                }
            } else {
                false
            }
        } else {
            false
        }
    }
}

pub struct TracesState {
    pub data: CallTraceArena,
    pub collapsed: Vec<bool>,
    pub active_call: ActiveCall,
}

impl TracesState {
    pub fn new(data: CallTraceArena) -> Self {
        let len = data.nodes().len();
        Self {
            data,
            collapsed: vec![false; len],
            active_call: Default::default(),
        }
    }

    /// Move selection down one step through the trace.
    /// Tries to step into, then next step, then parent's next step.
    pub fn down(&mut self) -> bool {
        loop {
            if !self.collapsed[self.active_call.idx]
                && self.active_call.step_into(self.data.nodes())
            {
                break true;
            }
            if self.active_call.next_step(self.data.nodes()) {
                break true;
            }
            if !self.active_call.jump_to_parent_call_step(self.data.nodes()) {
                break false;
            }
        }
    }

    /// Move selection up one step through the trace.
    /// Handles collapsed nodes and call boundaries.
    pub fn up(&mut self) -> bool {
        log::trace!("active_call: {:?}", self.active_call);
        let nodes = self.data.nodes();
        if self.active_call.prev_step(nodes) {
            log::trace!("active_call: {:?}", self.active_call);
            if self.active_call.active_item.is_call_header()
                && !self.collapsed[self.active_call.idx]
            {
                self.active_call.lastest_item(nodes);
                return true;
            }

            return true;
        } else if !self.active_call.active_item.is_call_header() {
            self.active_call.active_item = ActiveItem::CallHeader;
            return true;
        }

        if self.active_call.jump_to_parent_call_step(nodes) {
            self.active_call.follow(nodes);
            true
        } else {
            false
        }
    }

    /// Jump to the first trace node
    pub fn first(&mut self) {
        self.active_call.jump_to_call(0);
    }

    /// Jump to the last item in the trace
    pub fn last(&mut self) {
        self.first();
        self.active_call.lastest_item(self.data.nodes());
    }

    /// Step into the current call (enter the call's body)
    pub fn step_into(&mut self) {
        self.active_call.step_into(self.data.nodes());
    }

    /// Step over to the next sibling call (skip the current call's children)
    pub fn step_over(&mut self) {
        let curr_node = &self.data.nodes()[self.active_call.idx];
        if let Some(parent) = curr_node.parent {
            let parent = &self.data.nodes()[parent];
            let child_idx = parent
                .children
                .iter()
                .position(|&idx| idx == self.active_call.idx)
                .expect("curr_idx not found in parent_node.children");
            let n_children = parent.children.len();
            if child_idx == n_children - 1 {
                return;
            }
            self.active_call
                .jump_to_call(parent.children[child_idx + 1]);
        }
    }

    /// Step over backwards to the previous sibling call
    pub fn reverse_step_over(&mut self) {
        let curr_node = &self.data.nodes()[self.active_call.idx];
        if let Some(parent) = curr_node.parent {
            let parent = &self.data.nodes()[parent];
            let child_idx = parent
                .children
                .iter()
                .position(|&idx| idx == self.active_call.idx)
                .expect("curr_idx not found in parent_node.children");
            if child_idx == 0 {
                return;
            }
            self.active_call
                .jump_to_call(parent.children[child_idx - 1]);
        }
    }

    /// Step out of the current call (jump to parent call)
    pub fn step_out(&mut self) {
        let nodes = self.data.nodes();
        self.active_call.jump_to_parent_call_step(nodes);
        self.active_call.follow(nodes);
    }

    /// Get the current node index
    pub fn curr_idx(&self) -> usize {
        self.active_call.idx
    }

    /// Get the current active item (call header, log item, or return value)
    pub fn active_item(&self) -> ActiveItem {
        self.active_call.active_item
    }

    /// Get the current call's address in the trace tree (e.g., "0.1.2")
    pub fn curr_address(&self) -> String {
        let mut addr = Vec::new();
        let mut idx = self.active_call.idx;
        while let Some(parent) = self.data.nodes()[idx].parent {
            let parent_node = &self.data.nodes()[parent];
            let child_idx = parent_node
                .children
                .iter()
                .position(|child_idx| *child_idx == idx)
                .expect("curr_idx not found in parent_node.children");
            addr.push(child_idx.to_string());

            idx = parent_node.idx;
        }
        addr.push("0".into());

        addr.reverse();
        addr.join(".")
    }

    /// Toggle the collapsed state of the current call node
    pub fn toggle_collapse(&mut self) {
        if self.active_call.active_item.is_call_header() {
            let idx = self.active_call.idx;
            self.collapsed[idx] = !self.collapsed[idx];
        }
    }

    /// Convert the trace state to formatted text for display
    pub fn to_text(&self, selection_style: Option<SelectionStyle>) -> eyre::Result<Text<'static>> {
        TraceTextWriter::new(selection_style).write_to_text(self)
    }

    /// Reset the state to initial values (used in tests)
    #[allow(dead_code)]
    fn reset(&mut self) {
        self.collapsed.fill(false);
        self.active_call = Default::default();
    }
}

#[cfg(test)]
mod tests {
    use log::LevelFilter;
    use ratatui::{
        style::Color,
        text::{Line, Text},
    };
    use revm_inspectors::tracing::CallTraceArena;
    use simplelog::WriteLogger;

    use crate::{SelectionStyle, TracesState, trace_writer::RETURN};

    /// Test helper for validating trace navigation
    struct NavigationTestHelper<'a> {
        state: TracesState,
        text: Text<'a>,
        /// Current step number in the navigation sequence
        step_count: usize,
    }

    impl<'a> NavigationTestHelper<'a> {
        fn new(state: TracesState) -> Self {
            let text = state.to_text(Some(SelectionStyle::default())).unwrap();
            Self {
                state,
                text,
                step_count: 0,
            }
        }

        fn down(&mut self) {
            self.state.down();
            self.refresh_text();
            self.step_count += 1;
        }

        fn up(&mut self) {
            self.state.up();
            self.refresh_text();
            self.step_count -= 1;
        }

        fn reset(&mut self) {
            self.state.reset();
            self.refresh_text();
            self.step_count = 0;
        }

        /// Find the index of the currently highlighted line
        fn curr_highlighted(&self) -> Option<usize> {
            self.text
                .lines
                .iter()
                .position(|line| matches!(line.style.bg, Some(Color::Gray)))
        }

        /// Get the expected line index based on current step count
        fn expected_idx(&self) -> usize {
            self.text
                .lines
                .iter()
                .enumerate()
                .filter(|(_, line)| !Self::is_unjumpable(line))
                .nth(self.step_count)
                .unwrap()
                .0
        }

        /// Count total navigable steps in the trace
        fn n_steps(&self) -> usize {
            self.text
                .lines
                .iter()
                .filter(|line| !Self::is_unjumpable(line))
                .count()
        }

        /// Check if a line is unjumpable (e.g., closing braces that aren't return values)
        fn is_unjumpable(line: &Line) -> bool {
            line.to_string().contains(RETURN) && !line.to_string().contains("Return")
        }

        /// Randomly collapse every 3rd node
        fn apply_random_collapse(&mut self) {
            for i in (1..self.state.collapsed.len()).step_by(3) {
                self.state.collapsed[i] = !self.state.collapsed[i];
            }
            self.refresh_text();
        }

        /// Verify that the highlighted line matches expectations
        fn assert_correct_position(&self, context: &str) {
            let curr_idx = self.curr_highlighted().expect("no highlighted line found");
            let expected = self.expected_idx();

            assert_eq!(
                curr_idx,
                expected,
                "Incorrect position after {steps} steps\n\
                 EXPECTED:\n{expected_line}\n\
                 HIGHLIGHTED:\n{highlighted_line}\n\n\
                 Context:\n{trace_context}\n\n\
                 Additional context: {context}",
                steps = self.step_count,
                expected_line = self.text.lines[expected],
                highlighted_line = self.text.lines[curr_idx],
                trace_context = self.format_trace_context(expected, curr_idx),
            );
        }

        fn refresh_text(&mut self) {
            self.text = self.state.to_text(Some(SelectionStyle::default())).unwrap();
        }

        fn format_trace_context(&self, expected: usize, curr: usize) -> String {
            let end_idx = (expected.max(curr) + 50).min(self.text.lines.len());
            self.text.lines[0..end_idx]
                .iter()
                .map(|line| {
                    let s = line.to_string();
                    s[..100.min(s.len())].to_string()
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
    }

    // ============================================================================
    // Tests
    // ============================================================================

    #[test]
    fn test_navigation_up_down() {
        // Setup
        let test_trace = include_str!("../../web/example_trace.json");
        let test_trace: CallTraceArena = serde_json::from_str(test_trace).unwrap();
        let _ = WriteLogger::init(LevelFilter::Trace, Default::default(), std::io::stdout());

        let mut helper = NavigationTestHelper::new(TracesState::new(test_trace));

        // Test: Initial state should be at first position
        helper.assert_correct_position("initial state");

        // Test: Navigate down through all steps
        for i in 1..helper.n_steps() {
            helper.down();
            helper.assert_correct_position(&format!("after {i} down steps"));
        }

        // Test: Navigate back up through all steps
        for i in 1..helper.n_steps() {
            helper.up();
            helper.assert_correct_position(&format!("after {i} up steps"));
        }

        println!("✓ Normal navigation test passed");

        // Test: Navigation with collapsed nodes
        let steps_before = helper.n_steps();
        println!("Steps before collapse: {steps_before}");

        helper.reset();
        helper.apply_random_collapse();

        let steps_after = helper.n_steps();
        println!("Steps after collapse: {steps_after}");

        // Navigate down with collapsed nodes
        for i in 1..helper.n_steps() {
            helper.down();
            helper.assert_correct_position(&format!("collapsed trace, after {i} down steps"));
        }

        // Navigate back up with collapsed nodes
        for i in 1..helper.n_steps() {
            helper.up();
            helper.assert_correct_position(&format!(
                "collapsed trace, after {} down and {i} up steps",
                helper.n_steps()
            ));
        }

        println!("✓ Collapsed navigation test passed");
    }
}
