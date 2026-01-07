#[cfg(feature = "crossterm")]
pub use crossterm::event::KeyCode;
use ratatui::text::Text;
#[cfg(feature = "ratzilla")]
pub use ratzilla::event::KeyCode;

use crate::tui::TuiError;

macro_rules! bindings {
    (
        $($action:ident => $(ch($($char:literal)+))? $(cr($($cr_kc:ident)+))? $(rz($($rz_kc:ident)+))? ,)+
    ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Action {
            $($action,)*
        }

        impl TryFrom<KeyCode> for Action {
            type Error = TuiError;

            fn try_from(value: KeyCode) -> Result<Self, Self::Error> {
                match value {
                    $($(
                        $(KeyCode::Char($char))|* => Ok(Action::$action),
                    )?)*
                    $($($(KeyCode::$cr_kc)|* => Ok(Action::$action),)?)*
                    _ => Err(TuiError::UnknownKeybindError)
                }
            }
        }

        impl Action {
            pub fn to_text() -> Text<'static> {
                let mut text = Text::default();

                let mut action_keybinds = Vec::new();
                $(
                    let action = Action::$action;
                    let common_keybinds: Vec<char> = vec![ $( $($char),* )? ];
                    let backend_specific_keybinds: Vec<KeyCode> = vec![ $( $(KeyCode::$cr_kc),* )? ];

                    let mut all_keybinds = Vec::new();
                    all_keybinds.extend(common_keybinds.iter().map(|c| if *c == ' ' { "Space".to_string() } else { c.to_string() }));
                    all_keybinds.extend(backend_specific_keybinds.iter().map(|c| format!("{c:?}")));
                    let stringified = all_keybinds.join(", ");

                    action_keybinds.push((format!("{action:?}"), stringified));
                )*

                let width = action_keybinds.iter().map(|(_, keybinds)| keybinds.len()).max().unwrap_or_default();
                action_keybinds.iter().for_each(|(action, keybinds)| {
                    text.push_line(format!("{keybinds:>width$} - {action}"));
                });

                text
            }
        }
    };
}

bindings! {
    Quit => ch('q'),
    StepOver => ch('j'),
    ReverseStepOver => ch('k'),
    StepOut => ch('h'),
    StepInto => ch('l'),
    ToggleCollapse => ch(' ') cr(Enter) rz(Enter),
    ScrollDown => ch('J'),
    ScrollUp => ch('K'),
    GoToTop => ch('g'),
    GoToBottom => ch('G'),
    Up => cr(Up) rz(Up),
    Down => cr(Down) rz(Down),
    ScrollLeft => cr(Left) rz(Left),
    ScrollRight => cr(Right) rz(Right),
    Help => ch('?'),
}
