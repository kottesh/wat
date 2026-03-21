//! UI Components for the differential rendering system

mod error;
mod response;
mod tool_call;
mod tool_result;
mod user_input;

pub use error::ErrorComponent;
pub use response::ResponseComponent;
pub use tool_call::ToolCallComponent;
pub use tool_result::ToolResultComponent;
pub use user_input::UserInputComponent;
