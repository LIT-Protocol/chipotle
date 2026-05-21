use log::Level;
use serde_json::{Map, Value};

#[allow(unused_imports)]
use crate::error::unexpected::Unexpected;

pub trait LogRecord {
    fn fields(&self) -> &Map<String, Value>;
    fn debug_args(&self) -> &dyn std::fmt::Debug;
    fn display_args(&self) -> &dyn std::fmt::Display;
    fn level(&self) -> Level;
    fn target(&self) -> &str;
    fn name(&self) -> Option<&str>;
}
