use lit_core::config::LitConfig;
use std::backtrace::Backtrace;
use std::{fmt, panic};
use tracing::error;

use env_logger::fmt::{Color, Style, StyledValue};
use log::Level;
use serde_json::{Map, Value};

use lit_core::logging::plugin::Plugin;
use lit_core::utils::backtrace::{backtrace_to_vec, extract_panic_msg};

use crate::error::{Error, Kind, PKG_NAME, Result};

pub mod config;
pub mod error;
#[cfg(feature = "service")]
pub mod service;

pub struct Builder<'b> {
    #[allow(dead_code)]
    cfg: &'b LitConfig,
    #[allow(dead_code)]
    pkg: String,
    plugins: Vec<Box<dyn Plugin>>,
    fields: Option<Map<String, Value>>,
    identifier: Option<String>,
}

impl<'b> Builder<'b> {
    fn new(cfg: &'b LitConfig, pkg: &'static str) -> Builder<'b> {
        Self { cfg, pkg: pkg.to_string(), plugins: Vec::new(), fields: None, identifier: None }
    }

    pub fn plugin(mut self, plugin: impl Plugin + 'static) -> Builder<'b> {
        self.plugins.push(Box::new(plugin));
        self
    }

    /// Add some fields to the structured logs.
    pub fn add_field<K>(mut self, key: K, value: Value) -> Builder<'b>
    where
        K: Into<String>,
    {
        let mut fields = match self.fields.take() {
            Some(v) => v,
            None => Map::new(),
        };

        fields.insert(key.into(), value);
        self.fields = Some(fields);
        self
    }

    pub fn add_identifier(mut self, identifier: String) -> Builder<'b> {
        self.identifier = Some(identifier);
        self
    }

    #[deprecated(note = "This tool is deprecated, use `lit-observability` instead.")]
    pub fn init(self) -> Result<()> {
        Ok(())
    }
}

pub fn builder<'b>(cfg: &'b LitConfig, pkg: &'static str) -> Builder<'b> {
    Builder::new(cfg, pkg)
}

pub fn set_panic_hook() {
    panic::set_hook(Box::new(move |e| {
        let msg = extract_panic_msg(e);
        let backtrace = Backtrace::force_capture();
        let backtrace_vec = backtrace_to_vec(&backtrace);
        // let backtrace = format!("{backtrace}");

        // goes through the backtrace and removes lines with "/rustc/" and looks for the lines that have .rs files in them, to make finding the root cause easier.
        let mut filtered: Vec<String> = Vec::new();
        let mut prev_s: Option<String> = None;
        for s in &backtrace_vec {
            if !s.contains("/rustc/")
                && !s.contains("/.cargo/")
                && !s.contains("/lit-core/lit-logging/")
                && s.contains(".rs")
            {
                if let Some(prev) = prev_s {
                    filtered.push(prev);
                }
                filtered.push(s.clone());
            }
            prev_s = Some(s.clone());
        }
        let filtered = filtered.join("\n");

        let source: Option<String> = None;
        let err =
            Error::new(Some(Kind::Unexpected), PKG_NAME, Some(msg.clone()), None, source, None);

        // We log the backtrace to stderr and (if enabled) also to the tracing crate.
        eprintln!(
            "Unexpectedly panicked!: {}\nFull error: {}\nFiltered backtrace: \n{}\nFull backtrace:{}",
            msg,
            err,
            filtered,
            backtrace_vec.join("\n")
        );
        if tracing::enabled!(tracing::Level::ERROR) {
            error!(
                message = %msg,
                error = %err,
                filtered_backtrace = %filtered,
                backtrace = %backtrace_vec.join("\n"),
                "Unexpectedly panicked!"
            );
        }
    }));
}

pub struct Padded<T> {
    pub value: T,
    pub width: usize,
}

impl<T: fmt::Display> fmt::Display for Padded<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{: <width$}", self.value, width = self.width)
    }
}

pub fn colored_level(style: &mut Style, level: Level) -> StyledValue<'_, &'static str> {
    match level {
        Level::Trace => style.set_color(Color::Magenta).value("TRACE"),
        Level::Debug => style.set_color(Color::Blue).value("DEBUG"),
        Level::Info => style.set_color(Color::Green).value("INFO "),
        Level::Warn => style.set_color(Color::Yellow).value("WARN "),
        Level::Error => style.set_color(Color::Red).value("ERROR"),
    }
}

pub fn fields_to_json(fields: &Map<String, Value>) -> String {
    let mut s = String::new();

    if fields.is_empty() {
        return s;
    }

    s.push_str(" {");

    let len = fields.len();
    for (seq, (key, value)) in fields.iter().enumerate() {
        s.push_str(format!(" \"{key}\":{value}").as_str());
        if seq < (len - 1) {
            s.push(',');
        }
    }

    // Add space at the end as every item has a " " at the start.
    s.push_str(" }");

    s
}
