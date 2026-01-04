use ratatui::text::Text;
use revm_inspectors::tracing::{
    CallTraceArena,
    types::{CallTraceNode, TraceMemberOrder},
};

use crate::trace_writer::TraceTextWriter;

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
    fn jump_to_call(&mut self, idx: usize) {
        self.idx = idx;
        self.active_item = ActiveItem::CallHeader;
    }

    fn jump_to_call_item(&mut self, idx: usize, item_idx: usize) {
        self.idx = idx;
        self.active_item = ActiveItem::Item(item_idx);
    }

    /// Mutate into call header if current item is a call
    fn follow(&mut self, nodes: &[CallTraceNode]) -> bool {
        let curr_node = &nodes[self.idx];
        if let ActiveItem::Item(idx) = self.active_item {
            let item = &curr_node.ordering[idx];
            if let TraceMemberOrder::Call(idx) = item {
                self.jump_to_call(curr_node.children[*idx]);
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Mutate current call into an item of the parent call
    fn jump_to_parent_call_step(&mut self, nodes: &[CallTraceNode]) -> bool {
        if let Some(parent) = nodes[self.idx].parent {
            let parent = &nodes[parent];
            let child_idx = parent
                .children
                .iter()
                .position(|&idx| idx == self.idx)
                .expect("curr_idx not found in parent_node.children");
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

    /// Moves to the next sibling
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

    pub fn to_text(&self, highlight_active: bool) -> eyre::Result<Text<'static>> {
        TraceTextWriter::new(highlight_active).write_to_text(self)
    }

    pub fn down(&mut self) -> bool {
        loop {
            if !self.collapsed[self.active_call.idx]
                && self.active_call.step_into(&self.data.nodes())
            {
                break true;
            }
            if self.active_call.next_step(&self.data.nodes()) {
                break true;
            }
            if !self
                .active_call
                .jump_to_parent_call_step(&self.data.nodes())
            {
                break false;
            }
        }
    }

    fn reset(&mut self) {
        self.active_call = Default::default();
        self.collapsed.fill(false);
    }

    fn node(&self, idx: usize) -> &CallTraceNode {
        &self.data.nodes()[idx]
    }
    pub fn up(&mut self) -> bool {
        false
    }
    pub fn last(&mut self) {}
    pub fn first(&mut self) {}
    pub fn step_into(&mut self) {}
    pub fn step_over(&mut self) {}
    pub fn reverse_step_over(&mut self) {}
    pub fn step_out(&mut self) {}
    pub fn toggle_collapse(&mut self) {}
    pub fn curr_idx(&self) -> usize {
        0
    }
    pub fn curr_address(&self) -> String {
        String::new()
    }
    pub fn order_idx(&self) -> usize {
        0
    }

    // /// Step into the first child of the current call node
    // /// Returns true if it was able to step in, i.e. if the call had any children.
    // pub fn step_into(&mut self) -> bool {
    //     // self.curr_node().children.first().map(|idx| {
    //
    //     let curr_node = self.node(self.curr_call_idx);
    //     if !curr_node.children.is_empty() {
    //         self.curr_call_idx = curr_node.children[0];
    //         true
    //     } else {
    //         false
    //     }
    // }
    //
    // /// Step over the current call node i.e. move to the immediate next sibling of current call
    // /// Returns true if it was able to step over, i.e. if the call had any immediate next call
    // /// sibling.
    // pub fn step_over(&mut self) -> bool {
    //     self.move_call(true)
    // }
    //
    // /// Step over the current call node i.e. move to the immediate previous sibling of current
    // /// call.
    // /// Returns true if it was able to step over, i.e. if the call had any immediate previous call
    // /// sibling.
    // pub fn reverse_step_over(&mut self) -> bool {
    //     self.move_call(false)
    // }
    //
    // fn move_call(&mut self, forward: bool) -> bool {
    //     log::trace!("move_call: {forward}");
    //     let curr_node = self.curr_node();
    //     if let Some(parent) = curr_node.parent {
    //         let parent_node = self.node(parent);
    //         let n_children = parent_node.children.len();
    //         let curr_child_idx = parent_node
    //             .children
    //             .iter()
    //             .position(|&idx| idx == self.curr_call_idx)
    //             .expect("curr_idx not found in parent_node.children");
    //         if forward {
    //             if curr_child_idx == n_children - 1 {
    //                 false
    //             } else {
    //                 self.curr_call_idx = parent_node.children[curr_child_idx + 1];
    //                 self.order_idx = OrderingIndex::TopLevelCall;
    //                 true
    //             }
    //         } else {
    //             #[allow(clippy::collapsible_else_if)]
    //             if curr_child_idx == 0 {
    //                 false
    //             } else {
    //                 self.curr_call_idx = parent_node.children[curr_child_idx - 1];
    //                 self.order_idx = OrderingIndex::TopLevelCall;
    //                 true
    //             }
    //         }
    //     } else {
    //         false
    //     }
    // }
    //
    // fn move_step(&mut self, forward: bool) -> bool {
    //     log::trace!("move_step: {forward}, {:?}", self.order_idx);
    //     let curr_node = self.curr_node();
    //     let (curr_step_idx, search_node) = match self.order_idx {
    //         OrderingIndex::Item(order_idx) => (order_idx, self.curr_node()),
    //         OrderingIndex::TopLevelCall => {
    //             if let Some(parent) = curr_node.parent {
    //                 let parent_node = self.node(parent);
    //                 let child_idx = parent_node
    //                     .children
    //                     .iter()
    //                     .position(|&idx| idx == self.curr_call_idx)
    //                     .expect("curr_idx not found in parent_node.children");
    //                 let curr_step_idx = parent_node
    //                     .ordering
    //                     .iter()
    //                     .position(|item| *item == TraceMemberOrder::Call(child_idx))
    //                     .expect("curr_child not found in ordering");
    //                 (curr_step_idx, parent_node)
    //             } else {
    //                 // cant move any steps at root node
    //                 return false;
    //             }
    //         }
    //         OrderingIndex::ReturningValue => {
    //             if forward {
    //                 // cant move forward from returning value
    //                 return false;
    //             } else {
    //                 (curr_node.ordering.len(), curr_node)
    //             }
    //         }
    //     };
    //     let n_steps = search_node.ordering.len();
    //     log::trace!(
    //         "n_steps: {n_steps}, curr_step_idx: {curr_step_idx} search_node: {}",
    //         search_node.idx
    //     );
    //     if forward {
    //         if n_steps > 0 && curr_step_idx == n_steps - 1 {
    //             if is_returning_node(search_node) {
    //                 self.curr_call_idx = search_node.idx;
    //                 self.order_idx = OrderingIndex::ReturningValue;
    //                 true
    //             } else {
    //                 false
    //             }
    //         } else {
    //             self.curr_call_idx = search_node.idx;
    //             self.set_order_idx(curr_step_idx + 1);
    //             true
    //         }
    //     } else {
    //         #[allow(clippy::collapsible_else_if)]
    //         if curr_step_idx == 0 {
    //             false
    //         } else {
    //             self.curr_call_idx = search_node.idx;
    //             self.set_order_idx(curr_step_idx - 1);
    //             true
    //         }
    //     }
    // }
    //
    // /// Moves over to `curr_idx + idx`th sibling of the current item.
    // /// If `with_items` is true, it will move by that many indexes in the `node.ordering` array.
    // /// If `with_items` is false, it will move by that many indexes in the `node.children` array.
    // /// Returns true if it was able to move by that many indices.
    // fn delta_sibling_idx(&mut self, idx: isize, with_items: bool) -> bool {
    //     let curr_node = self.node(self.curr_call_idx);
    //
    //     if let Some(parent) = curr_node.parent {
    //         let parent_node = self.node(parent);
    //         let curr_child_idx = parent_node
    //             .children
    //             .iter()
    //             .position(|&idx| idx == self.curr_call_idx)
    //             .expect("step_over: curr_idx not found in parent_node.children");
    //         let curr_item_idx = parent_node
    //             .ordering
    //             .iter()
    //             .position(|item| *item == TraceMemberOrder::Call(curr_child_idx))
    //             .expect("curr_child_idx not found in ordering");
    //
    //         if with_items {
    //             let items_len = parent_node.ordering.len();
    //             let target_idx = curr_item_idx as isize + idx;
    //             match target_idx {
    //                 // invalid index, not possible
    //                 target_idx if target_idx < 0 || target_idx > items_len as isize => false,
    //                 target_idx if target_idx as usize == items_len => {
    //                     // if the parent call is returning value(RETURN, REVERT), step to that
    //                     if is_returning_node(parent_node) {
    //                         self.curr_call_idx = parent_node.idx;
    //                         self.order_idx = OrderingIndex::ReturningValue;
    //                         true
    //                     } else {
    //                         false
    //                     }
    //                 }
    //                 _ => {
    //                     self.curr_call_idx = parent_node.idx;
    //                     self.set_order_idx(target_idx as usize);
    //                     true
    //                 }
    //             }
    //         } else {
    //             let len = parent_node.children.len();
    //             let target_idx = curr_child_idx as isize + idx;
    //             if target_idx < 0 || target_idx >= len as isize {
    //                 false
    //             } else {
    //                 self.curr_call_idx = parent_node.children[target_idx as usize];
    //                 true
    //             }
    //         }
    //     } else {
    //         // can't step over at root node
    //         false
    //     }
    // }
    //
    // /// Step out of the current call node i.e. move to the parent of the current node
    // /// Returns true if it was able to step out, i.e. if the call had any parent.
    // pub fn step_out(&mut self) -> bool {
    //     log::trace!("step_out");
    //     let curr_node = self.node(self.curr_call_idx);
    //     if let Some(parent) = curr_node.parent {
    //         self.curr_call_idx = parent;
    //         self.order_idx = OrderingIndex::TopLevelCall;
    //         log::trace!(" {} {:?}", self.curr_call_idx, self.order_idx);
    //         true
    //     } else {
    //         false
    //     }
    // }
    //
    // /// Toggle the collapsed state of the current call node
    // pub fn toggle_collapse(&mut self) {
    //     if self.order_idx.is_top_level_call() {
    //         let idx = self.curr_call_idx;
    //         self.collapsed[idx] = !self.collapsed[idx];
    //     }
    // }
    //
    // fn curr_node(&self) -> &CallTraceNode {
    //     &self.data.nodes()[self.curr_call_idx]
    // }
    //
    // /// keeps going up the parent node until reaching root or until delta sibling idx succeeds
    // /// NOTE: doesnt really make sense to have arbitrary idx here tbh
    // ///
    // /// this call is useful when going from deepest item of a call to next call which might not
    // /// be on same level, so something like
    // /// A
    // ///  -> B
    // ///      -> C
    // ///          -> D
    // ///  -> E
    // /// So this call will be used to go from D to E
    // fn delta_sibling_idx_or_step_over_recurse(&mut self, idx: isize) -> bool {
    //     while !self.delta_sibling_idx(idx, true) {
    //         if let Some(parent) = self.curr_node().parent {
    //             self.curr_call_idx = parent;
    //             self.order_idx = OrderingIndex::TopLevelCall;
    //         } else {
    //             return false;
    //         }
    //     }
    //     true
    // }
    //
    // fn move_to_next_nearest_ancestor(&mut self) -> bool {
    //     self.order_idx = OrderingIndex::TopLevelCall;
    //     while !self.move_step(true) {
    //         if !self.step_out() {
    //             return false;
    //         }
    //     }
    //     true
    // }
    //
    // fn set_order_idx(&mut self, order_idx: usize) {
    //     let ordering = self.curr_node().ordering[order_idx];
    //     log::trace!("set_order_idx: {order_idx} {:?}", ordering);
    //     match ordering {
    //         TraceMemberOrder::Call(idx) => {
    //             self.curr_call_idx = self.curr_node().children[idx];
    //             self.order_idx = OrderingIndex::TopLevelCall;
    //         }
    //         _ => self.order_idx = OrderingIndex::Item(order_idx),
    //     }
    // }
    //
    // /// Returns true if move to return value was successful. The call has to return
    // fn to_returning_value(&mut self) -> bool {
    //     if let Some(ret_val) = self.curr_node().trace.status
    //         && (ret_val as u8) == 2
    //     {
    //         self.order_idx = OrderingIndex::ReturningValue;
    //         true
    //     } else {
    //         false
    //     }
    // }
    //
    // /// Returns true if next item move was successful
    // // fn next_item(&mut self) -> bool {
    // //     let items_len = self.curr_node().ordering.len();
    // //     match self.order_idx {
    // //         OrderingIndex::Item(order_idx) => {}
    // //         OrderingIndex::TopLevelCall => self.set_order_idx(0),
    // //         _ => false,
    // //     }
    // // }
    //
    // // pub fn down(&mut self) -> bool {
    // //     log::trace!("down {} {:?}", self.curr_call_idx, self.order_idx);
    // //     let curr_collapsed = self.collapsed[self.curr_call_idx];
    // //     if curr_collapsed {
    // //         return self.move_call(true);
    // //     }
    // //
    // //     let curr_node = self.curr_node();
    // //     let items_len = curr_node.ordering.len();
    // //
    // //     // step into the first item or returning value if available and if not collapsed
    // //     if matches!(self.order_idx, OrderingIndex::TopLevelCall)
    // //         && (items_len != 0 || is_returning_node(curr_node))
    // //         && !self.collapsed[self.curr_call_idx]
    // //     {
    // //         if items_len != 0 {
    // //             self.set_order_idx(0);
    // //             return true;
    // //         } else {
    // //             self.order_idx = OrderingIndex::ReturningValue;
    // //             return true;
    // //         }
    // //     }
    // //
    // //     if self.move_step(true) {
    // //         true
    // //     } else {
    // //         self.move_to_next_nearest_ancestor()
    // //     }
    // // }
    //
    // pub fn lastest_item(&mut self) -> bool {
    //     while let Some(TraceMemberOrder::Call(idx)) = self.curr_node().ordering.last() {
    //         self.curr_call_idx = self.curr_node().children[*idx];
    //     }
    //
    //     if is_returning_node(self.curr_node()) {
    //         self.order_idx = OrderingIndex::ReturningValue;
    //     } else {
    //         let items_len = self.curr_node().ordering.len();
    //         if items_len > 0 {
    //             self.set_order_idx(items_len - 1);
    //         }
    //     }
    //     true
    //
    //     // if self.order_idx().is_top_level_call() {
    //     //     // recursively go to the last uncollapsed child call's last item
    //     //     let curr_node = self.curr_node();
    //     //     if let Some(last_item) = curr_node.ordering.last() {
    //     //         match last_item {
    //     //             TraceMemberOrder::Call(idx) => {
    //     //                 let idx = self.curr_node().children[*idx];
    //     //                 self.curr_call_idx = idx;
    //     //                 if !self.node(idx).ordering.is_empty() {
    //     //                     self.lastest_item();
    //     //                 }
    //     //                 // while !self.collapsed[idx] && !self.node(idx).children.is_empty() {
    //     //                 //     idx = self.node(idx).children[self.node(idx).children.len() - 1];
    //     //                 // }
    //     //                 //
    //     //                 // let node = &self.data.nodes()[idx];
    //     //                 // if !self.collapsed[idx] && !node.ordering.is_empty() {
    //     //                 //     self.set_order_idx(node.ordering.len() - 1);
    //     //                 // }
    //     //                 // self.curr_idx = idx;
    //     //                 true
    //     //             }
    //     //             TraceMemberOrder::Log(idx) => {
    //     //                 self.set_order_idx(*idx);
    //     //                 true
    //     //             }
    //     //             _ => false,
    //     //         }
    //     //     } else {
    //     //         false
    //     //     }
    //     // } else {
    //     //     false
    //     // }
    // }
    //
    // pub fn up(&mut self) -> bool {
    //     if let OrderingIndex::Item(order_idx) = self.order_idx {
    //         if order_idx == 0 {
    //             // no more items, step over to the call
    //             self.order_idx = OrderingIndex::TopLevelCall;
    //             true
    //         } else {
    //             let ret = self.move_step(false);
    //             if self.order_idx == OrderingIndex::TopLevelCall {
    //                 // go to the lastest element
    //                 if is_returning_node(self.curr_node()) {
    //                     self.order_idx = OrderingIndex::ReturningValue;
    //                 } else {
    //                     self.lastest_item();
    //                 }
    //                 true
    //             } else {
    //                 ret
    //             }
    //         }
    //     } else if self.move_step(false) {
    //         if self.order_idx.is_top_level_call() {
    //             self.lastest_item();
    //         }
    //         true
    //     } else if let Some(parent) = self.curr_node().parent {
    //         self.curr_call_idx = parent;
    //         self.order_idx = OrderingIndex::TopLevelCall;
    //         true
    //     } else {
    //         false
    //     }
    // }
    //
    // pub fn curr_idx(&self) -> usize {
    //     self.curr_call_idx
    // }
    //
    // pub fn order_idx(&self) -> OrderingIndex {
    //     self.order_idx
    // }
    //
    // pub fn curr_address(&self) -> String {
    //     let mut addr = Vec::new();
    //     let mut idx = self.curr_call_idx;
    //     while let Some(parent) = self.data.nodes()[idx].parent {
    //         let parent_node = &self.data.nodes()[parent];
    //         let child_idx = parent_node
    //             .children
    //             .iter()
    //             .position(|child_idx| *child_idx == idx)
    //             .expect("curr_idx not found in parent_node.children");
    //         addr.push(child_idx.to_string());
    //
    //         idx = parent_node.idx;
    //     }
    //     addr.push("0".into());
    //
    //     addr.reverse();
    //     addr.join(".")
    // }
    //
    // pub fn first(&mut self) {
    //     self.curr_call_idx = 0;
    //     self.order_idx = OrderingIndex::TopLevelCall;
    // }
    //
    // pub fn last(&mut self) {
    //     self.first();
    //     self.lastest_item();
    // }
    //
    // fn reset(&mut self) {
    //     self.collapsed.fill(false);
    //     self.curr_call_idx = 0;
    //     self.order_idx = OrderingIndex::TopLevelCall;
    // }
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

    use crate::{TracesState, trace_writer::RETURN};

    #[test]
    fn test_down() {
        let test_trace = include_str!("../../web/example_trace.json");
        let test_trace: CallTraceArena = serde_json::from_str(test_trace).unwrap();

        WriteLogger::init(LevelFilter::Trace, Default::default(), std::io::stdout()).unwrap();

        struct Helper<'a> {
            state: TracesState,
            text: Text<'a>,
            idx: usize,
        }

        impl<'a> Helper<'a> {
            fn new(state: TracesState) -> Self {
                let text = state.to_text(true).unwrap();
                Self {
                    state,
                    text,
                    idx: 0,
                }
            }

            fn down(&mut self) {
                self.state.down();
                self.text = self.state.to_text(true).unwrap();
                self.idx += 1;
            }

            fn up(&mut self) {
                // self.state.up();
                self.text = self.state.to_text(true).unwrap();
                self.idx -= 1;
            }

            fn curr_highlighted(&self) -> Option<usize> {
                self.text
                    .lines
                    .iter()
                    .position(|line| matches!(line.style.bg, Some(Color::Gray)))
            }

            fn expected_idx(&self) -> usize {
                self.text
                    .lines
                    .iter()
                    .enumerate()
                    .filter(|(_, line)| !Self::is_unjumpable(line))
                    .nth(self.idx)
                    .unwrap()
                    .0
            }

            fn assert(&self, additional_ctx: &str) {
                let curr_idx = self.curr_highlighted().expect("cant find curr highlighted");
                let expected = self.expected_idx();
                assert_eq!(
                    curr_idx,
                    expected,
                    "incorrect index after {down} down\nEXPECTED:\n{expected}\nHIGHLIGHTED:\n{highlighted}\n\ncontext:\n\n{context}\nadditional context:\n{additional_ctx}",
                    down = self.idx,
                    expected = self.text.lines[expected],
                    highlighted = self.text.lines[curr_idx],
                    context = self.text.lines
                        [0..(expected.max(curr_idx) + 10).min(self.text.lines.len())]
                        .iter()
                        .map(|l| {
                            let str = l.to_string();
                            str.as_str()[..100.min(str.len())].to_string()
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
            }

            fn n_steps(&self) -> usize {
                self.text
                    .lines
                    .iter()
                    .filter(|line| !Self::is_unjumpable(line))
                    .count()
            }

            fn is_unjumpable(line: &Line) -> bool {
                line.to_string().contains(RETURN) && !line.to_string().contains("Return")
            }

            fn randomly_collapsed(&mut self) {
                for i in (1..self.state.collapsed.len()).step_by(3) {
                    self.state.collapsed[i] = !self.state.collapsed[i];
                }
                self.text = self.state.to_text(true).unwrap();
            }

            fn reset(&mut self) {
                self.state.reset();
                self.text = self.state.to_text(true).unwrap();
                self.idx = 0;
            }
        }

        let mut helper = Helper::new(TracesState::new(test_trace));
        helper.assert("no mutations");

        for i in 1..helper.n_steps() {
            helper.down();
            helper.assert(format!("down {i} times").as_str());
        }
        println!("normal down test successful");

        println!("before random collapse size: {}", helper.n_steps());
        helper.reset();
        helper.randomly_collapsed();
        println!("after random collapse size: {}", helper.n_steps());
        for i in 1..helper.n_steps() {
            helper.down();
            helper.assert(format!("[collapsed] down {i} times").as_str());
        }

        // for i in 1..helper.n_steps() {
        //     helper.up();
        //     helper.assert(format!("down {} times, up {i} times", helper.n_steps()).as_str());
        // }
    }
}
