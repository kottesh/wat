//! UI Components for the differential rendering system

#![allow(dead_code)]

mod user_input;
mod response;
mod tool_call;
mod tool_result;
mod spinner;
mod error;
mod prompt;
mod thinking;
mod separator;

pub use user_input::UserInputComponent;
pub use response::ResponseComponent;
pub use tool_call::ToolCallComponent;
pub use tool_result::ToolResultComponent;
pub use spinner::SpinnerComponent;
pub use error::ErrorComponent;
pub use prompt::PromptComponent;
pub use thinking::ThinkingComponent;
pub use separator::SeparatorComponent;
