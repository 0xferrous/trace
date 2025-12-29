use alloy_primitives::hex;
use foundry_evm_traces::CallTraceArena;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span, Text},
};
use revm::interpreter::InstructionResult;
use revm_inspectors::tracing::types::{
    CallKind, DecodedCallData, DecodedTraceStep, TraceMemberOrder,
};

const PIPE: &str = "  │ ";
const EDGE: &str = "  └─ ";
const BRANCH: &str = "  ├─ ";
const CALL: &str = "→ ";
const RETURN: &str = "← ";

const SYM_COLLAPSED: &str = "⏵";
const SYM_EXPANDED: &str = "⏷";

pub struct TracesState {
    data: CallTraceArena,
    collapsed: Vec<bool>,
    curr_idx: usize,
}

impl TracesState {
    pub fn new(data: CallTraceArena) -> Self {
        let len = data.nodes().len();
        Self {
            data,
            collapsed: vec![false; len],
            curr_idx: 0,
        }
    }

    pub fn to_text(&self) -> eyre::Result<Text<'static>> {
        TraceTextWriter::new().to_text(self)
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

    fn to_text(mut self, state: &TracesState) -> eyre::Result<Text<'static>> {
        self.write_node(state, 0)?;
        Ok(Text::from(self.lines))
    }

    fn trace_style(trace: &foundry_evm_traces::CallTrace) -> Style {
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
        let mut buf = String::from("  ");
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
                self.write_log(state, node_idx, *index)?;
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
        let sym = if is_collapsed { SYM_COLLAPSED } else { SYM_EXPANDED };
        spans.push(Span::raw(format!("  {sym} ")));

        // Trace header
        self.write_trace_header_spans(&mut spans, &node.trace)?;
        self.lines.push(Line::from(spans));

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
        trace: &foundry_evm_traces::CallTrace,
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
        trace: &foundry_evm_traces::CallTrace,
    ) -> eyre::Result<()> {
        let style = Self::trace_style(trace);

        spans.push(Span::styled(RETURN.to_string(), style));
        spans.push(Span::styled(
            format!("[{:?}]", trace.status.unwrap_or(InstructionResult::Stop)),
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

            spans.push(Span::styled(format!(" data: {}", log.raw_log.data), log_style));
        }

        self.lines.push(Line::from(spans));
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
                spans.push(Span::raw(format!("[{gas_used}] {}{args_str}", call.func_name)));
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
