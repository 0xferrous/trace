use alloy_primitives::hex;
use ratatui::{
    style::{Color, Style, Stylize},
    text::{Line, Span, Text},
};
use revm_inspectors::tracing::{
    CallTraceArena,
    types::{
        CallKind, CallTrace, CallTraceNode, DecodedCallData, DecodedTraceStep, TraceMemberOrder,
    },
};

const EMPTY: &str = "    ";
const PIPE: &str = "  │ ";
const EDGE: &str = "  └─ ";
const BRANCH: &str = "  ├─ ";
const CALL: &str = "→ ";
const RETURN: &str = "← ";

// const SYM_COLLAPSED: &str = "⏵";
// const SYM_COLLAPSED: &str = "⊟";
// const SYM_EXPANDED: &str = "⊞";
// const SYM_EXPANDED: &str = "⏷";

const SYM_COLLAPSED: &str = "◇";
const SYM_EXPANDED: &str = "◆";

pub struct TracesState {
    data: CallTraceArena,
    collapsed: Vec<bool>,
    curr_idx: usize,
    order_idx: Option<usize>,
}

impl TracesState {
    pub fn new(data: CallTraceArena) -> Self {
        let len = data.nodes().len();
        Self {
            data,
            collapsed: vec![false; len],
            curr_idx: 0,
            order_idx: None,
        }
    }

    pub fn to_text(&self) -> eyre::Result<Text<'static>> {
        TraceTextWriter::new().write_to_text(self)
    }

    fn node(&self, idx: usize) -> &CallTraceNode {
        &self.data.nodes()[idx]
    }

    /// Step into the first child of the current call node
    pub fn step_into(&mut self) -> bool {
        let curr_node = self.node(self.curr_idx);
        if !curr_node.children.is_empty() {
            self.curr_idx = curr_node.children[0];
            true
        } else {
            false
        }
    }

    /// Step over the current call node i.e. move to the immediate next sibling of current node
    pub fn step_over(&mut self) -> bool {
        self.delta_sibling_idx(1)
    }

    /// Step over the current call node i.e. move to the immediate previous sibling of current node
    pub fn reverse_step_over(&mut self) -> bool {
        self.delta_sibling_idx(-1)
    }

    fn delta_sibling_idx(&mut self, idx: isize) -> bool {
        let curr_node = self.node(self.curr_idx);

        if let Some(parent) = curr_node.parent {
            let parent_node = self.node(parent);
            let curr_child_idx = parent_node
                .children
                .iter()
                .position(|&idx| idx == self.curr_idx)
                .expect("step_over: curr_idx not found in parent_node.children");
            let len = parent_node.children.len();
            let target_idx = curr_child_idx as isize + idx;
            if target_idx < 0 || target_idx >= len as isize {
                false
            } else {
                self.curr_idx = parent_node.children[target_idx as usize];
                true
            }
        } else {
            // can't step over at root node
            false
        }
    }

    /// Step out of the current call node i.e. move to the parent of the current node
    pub fn step_out(&mut self) -> bool {
        let curr_node = self.node(self.curr_idx);
        if let Some(parent) = curr_node.parent {
            self.curr_idx = parent;
            true
        } else {
            false
        }
    }

    /// Toggle the collapsed state of the current call node
    pub fn toggle_collapse(&mut self) {
        let idx = self.curr_idx;
        self.collapsed[idx] = !self.collapsed[idx];
    }

    fn curr_node(&self) -> &CallTraceNode {
        &self.data.nodes()[self.curr_idx]
    }

    /// keeps going up the parent node until reaching root or until delta sibling idx succeeds
    fn delta_sibling_idx_or_step_over_recurse(&mut self, idx: isize) -> bool {
        while !self.delta_sibling_idx(idx) {
            if let Some(parent) = self.curr_node().parent {
                self.curr_idx = parent;
                self.order_idx = None;
            } else {
                return false;
            }
        }
        true
    }

    fn set_order_idx(&mut self, order_idx: usize) {
        let ordering = self.curr_node().ordering[order_idx];
        match ordering {
            TraceMemberOrder::Call(idx) => {
                self.curr_idx = self.curr_node().children[idx];
                self.order_idx = None;
            }
            _ => self.order_idx = Some(order_idx),
        }
    }

    pub fn down(&mut self) -> bool {
        let curr_collapsed = self.collapsed[self.curr_idx];
        if curr_collapsed {
            return self.delta_sibling_idx_or_step_over_recurse(1);
        }

        let curr_node = self.node(self.curr_idx);
        let items_len = curr_node.ordering.len();
        if let Some(order_idx) = self.order_idx {
            // scrolling through the items
            let next_idx = order_idx + 1;
            if next_idx < items_len {
                // go to next id
                self.set_order_idx(next_idx);
                true
            } else {
                self.delta_sibling_idx_or_step_over_recurse(1)
            }
        } else if items_len == 0 {
            // no items, step over
            self.delta_sibling_idx_or_step_over_recurse(1)
        } else {
            // some items, step into items
            self.set_order_idx(0);
            true
        }
    }

    pub fn up(&mut self) -> bool {
        if let Some(order_idx) = self.order_idx {
            if order_idx == 0 {
                // no more items, step over to the call
                self.order_idx = None;
                true
            } else {
                // step to previous item
                self.set_order_idx(order_idx - 1);
                true
            }
        } else if self.delta_sibling_idx(-1) {
            // recursively go to the last uncollapsed child call's last item
            let mut idx = self.curr_idx;

            while !self.collapsed[idx] && !self.node(idx).children.is_empty() {
                idx = self.node(idx).children[self.node(idx).children.len() - 1];
            }

            let node = &self.data.nodes()[idx];
            if !self.collapsed[idx] && !node.ordering.is_empty() {
                self.set_order_idx(node.ordering.len() - 1);
            }
            self.curr_idx = idx;

            true
        } else if let Some(parent) = self.curr_node().parent {
            self.curr_idx = parent;
            true
        } else {
            false
        }
    }

    pub fn curr_idx(&self) -> usize {
        self.curr_idx
    }

    pub fn order_idx(&self) -> Option<usize> {
        self.order_idx
    }

    pub fn curr_address(&self) -> String {
        let mut addr = Vec::new();
        let mut idx = self.curr_idx;
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
}

struct TraceTextWriter {
    lines: Vec<Line<'static>>,
    indentation_level: usize,
}

impl TraceTextWriter {
    fn new() -> Self {
        Self {
            lines: Vec::new(),
            indentation_level: 0,
        }
    }

    fn write_to_text(mut self, state: &TracesState) -> eyre::Result<Text<'static>> {
        self.write_node(state, 0)?;
        Ok(Text::from(self.lines))
    }

    fn trace_style(trace: &CallTrace) -> Style {
        let color = if trace.success {
            Color::Green
        } else {
            Color::Red
        };
        Style::default().fg(color)
    }

    fn trace_kind_style() -> Style {
        Style::default().fg(Color::Yellow)
    }

    fn log_style() -> Style {
        Style::default().fg(Color::Cyan)
    }

    fn make_indentation(&self) -> String {
        let mut buf = String::from("");
        if self.indentation_level > 0 {
            buf.push_str(EMPTY);
        }
        for _ in 1..self.indentation_level {
            buf.push_str(PIPE);
        }
        buf
    }

    fn write_item(
        &mut self,
        state: &TracesState,
        node_idx: usize,
        item_idx: usize,
    ) -> eyre::Result<usize> {
        let node = &state.data.nodes()[node_idx];
        match &node.ordering[item_idx] {
            TraceMemberOrder::Log(index) => {
                self.write_log(state, node_idx, *index, item_idx)?;
                Ok(item_idx + 1)
            }
            TraceMemberOrder::Call(index) => {
                self.write_node(state, node.children[*index])?;
                Ok(item_idx + 1)
            }
            TraceMemberOrder::Step(index) => self.write_step(state, node_idx, item_idx, *index),
        }
    }

    fn write_items_until(
        &mut self,
        state: &TracesState,
        node_idx: usize,
        first_item_idx: usize,
        f: impl Fn(usize) -> bool,
    ) -> eyre::Result<usize> {
        let mut item_idx = first_item_idx;
        while !f(item_idx) {
            item_idx = self.write_item(state, node_idx, item_idx)?;
        }
        Ok(item_idx)
    }

    fn write_items(&mut self, state: &TracesState, node_idx: usize) -> eyre::Result<()> {
        let items_cnt = state.data.nodes()[node_idx].ordering.len();
        self.write_items_until(state, node_idx, 0, |idx| idx == items_cnt)?;
        Ok(())
    }

    fn write_node(&mut self, state: &TracesState, idx: usize) -> eyre::Result<()> {
        let node = &state.data.nodes()[idx];
        let is_collapsed = state.collapsed[idx];

        // Write header with collapse indicator
        let mut spans = Vec::new();

        // Always add base indentation
        let indent = self.make_indentation();
        spans.push(Span::raw(indent));

        // All call nodes are collapsible (they all have at least a footer)
        // Replace "  ├─ " with "  ⏷ " or "  ⏵ " to align with branch position
        let sym = if is_collapsed {
            SYM_COLLAPSED
        } else {
            SYM_EXPANDED
        };
        spans.push(Span::raw(format!("  {sym} ")));

        // Trace header
        self.write_trace_header_spans(&mut spans, &node.trace)?;
        let mut line = Line::from(spans);
        if idx == state.curr_idx && state.order_idx.is_none() {
            line = line.on_gray();
        }
        self.lines.push(line);

        // Only write children and footer if not collapsed
        if !is_collapsed {
            self.indentation_level += 1;
            self.write_items(state, idx)?;

            // Write return footer
            let mut footer_spans = Vec::new();
            let indent = self.make_indentation();
            footer_spans.push(Span::raw(indent));
            footer_spans.push(Span::raw(EDGE.to_string()));
            self.write_trace_footer_spans(&mut footer_spans, &node.trace)?;
            self.lines.push(Line::from(footer_spans));

            self.indentation_level -= 1;
        }

        Ok(())
    }

    fn write_trace_header_spans(
        &self,
        spans: &mut Vec<Span<'static>>,
        trace: &CallTrace,
    ) -> eyre::Result<()> {
        let style = Self::trace_style(trace);
        let kind_style = Self::trace_kind_style();

        // Gas used
        spans.push(Span::raw(format!("[{}] ", trace.gas_used)));

        let address = trace.address.to_string();

        if trace.kind.is_any_create() {
            spans.push(Span::styled(CALL.to_string(), kind_style));
            spans.push(Span::styled("new ".to_string(), kind_style));
            let label = trace
                .decoded
                .as_ref()
                .and_then(|d| d.label.as_deref())
                .unwrap_or("<unknown>");
            spans.push(Span::styled(format!("{label}@{address}"), style));
        } else {
            let (func_name, inputs) =
                match trace.decoded.as_ref().and_then(|d| d.call_data.as_ref()) {
                    Some(DecodedCallData { signature, args }) => {
                        let name = signature.split('(').next().unwrap();
                        (name.to_string(), args.join(", "))
                    }
                    None => {
                        if trace.data.len() < 4 {
                            ("fallback".to_string(), hex::encode(&trace.data))
                        } else {
                            let (selector, data) = trace.data.split_at(4);
                            (hex::encode(selector), hex::encode(data))
                        }
                    }
                };

            let label = trace
                .decoded
                .as_ref()
                .and_then(|d| d.label.as_deref())
                .unwrap_or(&address);

            spans.push(Span::styled(format!("{label}::{func_name}"), style));

            if !trace.value.is_zero() {
                spans.push(Span::raw(format!("{{value: {}}}", trace.value)));
            }

            spans.push(Span::raw(format!("({inputs})")));

            let action = match trace.kind {
                CallKind::Call => None,
                CallKind::StaticCall => Some(" [staticcall]"),
                CallKind::CallCode => Some(" [callcode]"),
                CallKind::DelegateCall => Some(" [delegatecall]"),
                CallKind::AuthCall => Some(" [authcall]"),
                CallKind::Create | CallKind::Create2 => None,
            };
            if let Some(action) = action {
                spans.push(Span::styled(action.to_string(), kind_style));
            }
        }

        Ok(())
    }

    fn write_trace_footer_spans(
        &self,
        spans: &mut Vec<Span<'static>>,
        trace: &CallTrace,
    ) -> eyre::Result<()> {
        let style = Self::trace_style(trace);

        spans.push(Span::styled(RETURN.to_string(), style));
        spans.push(Span::styled(
            format!("[{:?}]", trace.status.unwrap_or_default()),
            style,
        ));

        if let Some(decoded) = trace
            .decoded
            .as_ref()
            .and_then(|d| d.return_data.as_deref())
        {
            spans.push(Span::styled(format!(" {decoded}"), style));
        } else if trace.kind.is_any_create() && trace.status.is_none_or(|status| status.is_ok()) {
            spans.push(Span::raw(format!(" {} bytes of code", trace.output.len())));
        } else if !trace.output.is_empty() {
            spans.push(Span::raw(format!(" {}", trace.output)));
        }

        Ok(())
    }

    fn write_log(
        &mut self,
        state: &TracesState,
        node_idx: usize,
        log_idx: usize,
        order_idx: usize,
    ) -> eyre::Result<()> {
        let node = &state.data.nodes()[node_idx];
        let log = &node.logs[log_idx];

        let mut spans = Vec::new();
        let log_style = Self::log_style();

        // Indentation
        let indent = self.make_indentation();
        spans.push(Span::raw(indent));
        spans.push(Span::raw(BRANCH.to_string()));

        if let Some(decoded) = log.decoded.as_ref() {
            let name = decoded.name.as_deref().unwrap_or("UnknownEvent");
            spans.push(Span::raw(format!("emit {name}(")));

            if let Some(params) = &decoded.params {
                let params_str = params
                    .iter()
                    .map(|(param_name, param_value)| format!("{param_name}: {param_value}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                spans.push(Span::styled(params_str, log_style));
            }

            spans.push(Span::raw(")".to_string()));
        } else {
            spans.push(Span::raw("emit ".to_string()));

            let mut topic_parts = Vec::new();
            for (i, topic) in log.raw_log.topics().iter().enumerate() {
                topic_parts.push(format!("topic{i}: {topic}"));
            }
            if !topic_parts.is_empty() {
                spans.push(Span::styled(topic_parts.join(", "), log_style));
            }

            spans.push(Span::styled(
                format!(" data: {}", log.raw_log.data),
                log_style,
            ));
        }

        let line = Line::from(spans);
        let line = if let Some(curr_order_idx) = state.order_idx
            && state.curr_idx == node_idx
            && order_idx == curr_order_idx
        {
            line.on_gray()
        } else {
            line
        };
        self.lines.push(line);
        Ok(())
    }

    fn write_step(
        &mut self,
        state: &TracesState,
        node_idx: usize,
        item_idx: usize,
        step_idx: usize,
    ) -> eyre::Result<usize> {
        let node = &state.data.nodes()[node_idx];
        let step = &node.trace.steps[step_idx];

        let Some(decoded) = &step.decoded else {
            return Ok(item_idx + 1);
        };

        match &**decoded {
            DecodedTraceStep::InternalCall(call, end_idx) => {
                let gas_used = node.trace.steps[*end_idx]
                    .gas_used
                    .saturating_sub(step.gas_used);

                let mut spans = Vec::new();
                let indent = self.make_indentation();
                spans.push(Span::raw(indent));
                spans.push(Span::raw(BRANCH.to_string()));
                self.indentation_level += 1;

                let args_str = call
                    .args
                    .as_ref()
                    .map(|v| format!("({})", v.join(", ")))
                    .unwrap_or_default();
                spans.push(Span::raw(format!(
                    "[{gas_used}] {}{args_str}",
                    call.func_name
                )));
                self.lines.push(Line::from(spans));

                let end_item_idx = self.write_items_until(state, node_idx, item_idx + 1, |item_idx| {
                    matches!(&node.ordering[item_idx], TraceMemberOrder::Step(idx) if *idx == *end_idx)
                })?;

                let mut footer_spans = Vec::new();
                let indent = self.make_indentation();
                footer_spans.push(Span::raw(indent));
                footer_spans.push(Span::raw(EDGE.to_string()));
                footer_spans.push(Span::raw(RETURN.to_string()));

                if let Some(outputs) = &call.return_data {
                    footer_spans.push(Span::raw(outputs.join(", ")));
                }

                self.lines.push(Line::from(footer_spans));
                self.indentation_level -= 1;

                Ok(end_item_idx + 1)
            }
            DecodedTraceStep::Line(line_text) => {
                let mut spans = Vec::new();
                let indent = self.make_indentation();
                spans.push(Span::raw(indent));
                spans.push(Span::raw(BRANCH.to_string()));
                spans.push(Span::raw(line_text.clone()));
                self.lines.push(Line::from(spans));

                Ok(item_idx + 1)
            }
        }
    }
}
