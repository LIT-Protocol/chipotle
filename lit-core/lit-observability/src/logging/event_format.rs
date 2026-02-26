//! This file copies the basic event formatting logic from here: https://github.com/tokio-rs/tracing/blob/acf92ab944f6fa0c9dee0302a334d4fdf0cfe52a/tracing-subscriber/src/fmt/format/mod.rs
//!
//! The `CustomEventFormatter` adds additional logic to customize the formatting of log messages, such as:
//! - Optionally omitting the event / span scopes
//! - Optionally adding a prefix string to each log message
//! - Displaying request_id and correlation_id from span extensions for request tracing
//!
//! This mod should mostly be used in development and testing environments.

#![allow(dead_code)]

use core::fmt;

#[cfg(feature = "tracing-log")]
use tracing_log::NormalizeEvent;

#[cfg(feature = "ansi")]
use nu_ansi_term::Color;
use nu_ansi_term::Style;

use tracing::{Event, Level, Subscriber};
use tracing_subscriber::{
    fmt::{
        FmtContext, FormatEvent, FormatFields, FormattedFields,
        format::Writer,
        time::{FormatTime, SystemTime},
    },
    registry::LookupSpan,
};

use super::context_layer::RequestContext;

/// A variant of the default formatter `tracing_subscriber::fmt::format::Format` that adds additional logic to customize the formatting of log messages, such as:
/// - Optionally omitting the event / span scopes
/// - Optionally adding a prefix string to each log message
/// - Displaying request_id and correlation_id from span extensions
///
/// This struct is mostly used in development and testing environments.
#[derive(Debug, Clone)]
pub struct CustomEventFormatter<T = SystemTime> {
    pub(crate) timer: T,
    pub(crate) ansi: Option<bool>,
    pub(crate) display_timestamp: bool,
    pub(crate) display_target: bool,
    pub(crate) display_level: bool,
    pub(crate) display_thread_id: bool,
    pub(crate) display_thread_name: bool,
    pub(crate) display_filename: bool,
    pub(crate) display_line_number: bool,
    pub(crate) display_event_scope: bool,
    pub(crate) display_request_context: bool,
    pub(crate) prefix_string: Option<String>,
}

impl Default for CustomEventFormatter {
    fn default() -> Self {
        Self {
            timer: SystemTime,
            ansi: None,
            display_timestamp: true,
            display_target: true,
            display_level: true,
            display_thread_id: false,
            display_thread_name: false,
            display_filename: false,
            display_line_number: false,
            display_event_scope: true,
            display_request_context: true,
            prefix_string: None,
        }
    }
}

impl<T> CustomEventFormatter<T> {
    /// Use the given [`timer`] for log message timestamps.
    ///
    /// See [`time` module] for the provided timer implementations.
    ///
    /// Note that using the `"time"` feature flag enables the
    /// additional time formatters [`UtcTime`] and [`LocalTime`], which use the
    /// [`time` crate] to provide more sophisticated timestamp formatting
    /// options.
    ///
    /// [`timer`]: super::time::FormatTime
    /// [`time` module]: mod@super::time
    /// [`UtcTime`]: super::time::UtcTime
    /// [`LocalTime`]: super::time::LocalTime
    /// [`time` crate]: https://docs.rs/time/0.3
    pub fn with_timer<T2>(self, timer: T2) -> CustomEventFormatter<T2> {
        CustomEventFormatter {
            timer,
            ansi: self.ansi,
            display_target: self.display_target,
            display_timestamp: self.display_timestamp,
            display_level: self.display_level,
            display_thread_id: self.display_thread_id,
            display_thread_name: self.display_thread_name,
            display_filename: self.display_filename,
            display_line_number: self.display_line_number,
            display_event_scope: self.display_event_scope,
            display_request_context: self.display_request_context,
            prefix_string: self.prefix_string,
        }
    }

    /// Do not emit timestamps with log messages.
    pub fn without_time(self) -> CustomEventFormatter<()> {
        CustomEventFormatter {
            timer: (),
            ansi: self.ansi,
            display_timestamp: false,
            display_target: self.display_target,
            display_level: self.display_level,
            display_thread_id: self.display_thread_id,
            display_thread_name: self.display_thread_name,
            display_filename: self.display_filename,
            display_line_number: self.display_line_number,
            display_event_scope: self.display_event_scope,
            display_request_context: self.display_request_context,
            prefix_string: self.prefix_string,
        }
    }

    /// Sets whether or not request context (request_id, correlation_id) is displayed.
    ///
    /// When enabled, the formatter will look for `RequestContext` in span extensions
    /// and display the request_id and correlation_id if present.
    pub fn with_request_context(self, display_request_context: bool) -> CustomEventFormatter<T> {
        CustomEventFormatter { display_request_context, ..self }
    }

    /// Enable ANSI terminal colors for formatted output.
    pub fn with_ansi(self, ansi: bool) -> CustomEventFormatter<T> {
        Self { ansi: Some(ansi), ..self }
    }

    /// Sets whether or not an event's target is displayed.
    pub fn with_target(self, display_target: bool) -> CustomEventFormatter<T> {
        CustomEventFormatter { display_target, ..self }
    }

    /// Sets whether or not an event's level is displayed.
    pub fn with_level(self, display_level: bool) -> CustomEventFormatter<T> {
        CustomEventFormatter { display_level, ..self }
    }

    /// Sets whether or not the [thread ID] of the current thread is displayed
    /// when formatting events.
    ///
    /// [thread ID]: std::thread::ThreadId
    pub fn with_thread_ids(self, display_thread_id: bool) -> CustomEventFormatter<T> {
        CustomEventFormatter { display_thread_id, ..self }
    }

    /// Sets whether or not the [name] of the current thread is displayed
    /// when formatting events.
    ///
    /// [name]: std::thread#naming-threads
    pub fn with_thread_names(self, display_thread_name: bool) -> CustomEventFormatter<T> {
        CustomEventFormatter { display_thread_name, ..self }
    }

    /// Sets whether or not an event's [source code file path][file] is
    /// displayed.
    ///
    /// [file]: tracing_core::Metadata::file
    pub fn with_file(self, display_filename: bool) -> CustomEventFormatter<T> {
        CustomEventFormatter { display_filename, ..self }
    }

    /// Sets whether or not an event's [source code line number][line] is
    /// displayed.
    ///
    /// [line]: tracing_core::Metadata::line
    pub fn with_line_number(self, display_line_number: bool) -> CustomEventFormatter<T> {
        CustomEventFormatter { display_line_number, ..self }
    }

    /// Sets whether or not the scope of the event is displayed.
    pub fn with_event_scope(self, display_event_scope: bool) -> CustomEventFormatter<T> {
        CustomEventFormatter { display_event_scope, ..self }
    }

    /// When set, the provided string will be printed before each log message.
    pub fn with_prefix_string(self, prefix_string: Option<String>) -> CustomEventFormatter<T> {
        CustomEventFormatter { prefix_string, ..self }
    }

    /// Sets whether or not the source code location from which an event
    /// originated is displayed.
    ///
    /// This is equivalent to calling [`Format::with_file`] and
    /// [`Format::with_line_number`] with the same value.
    pub fn with_source_location(self, display_location: bool) -> Self {
        self.with_line_number(display_location).with_file(display_location)
    }

    #[inline]
    fn format_timestamp(&self, writer: &mut Writer<'_>) -> fmt::Result
    where
        T: FormatTime,
    {
        // If timestamps are disabled, do nothing.
        if !self.display_timestamp {
            return Ok(());
        }

        // If ANSI color codes are enabled, format the timestamp with ANSI
        // colors.
        #[cfg(feature = "ansi")]
        {
            if writer.has_ansi_escapes() {
                let style = Style::new().dimmed();
                write!(writer, "{}", style.prefix())?;

                // If getting the timestamp failed, don't bail --- only bail on
                // formatting errors.
                if self.timer.format_time(writer).is_err() {
                    writer.write_str("<unknown time>")?;
                }

                write!(writer, "{} ", style.suffix())?;
                return Ok(());
            }
        }

        // Otherwise, just format the timestamp without ANSI formatting.
        // If getting the timestamp failed, don't bail --- only bail on
        // formatting errors.
        if self.timer.format_time(writer).is_err() {
            writer.write_str("<unknown time>")?;
        }
        writer.write_char(' ')
    }
}

impl<S, N, T> FormatEvent<S, N> for CustomEventFormatter<T>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
    T: FormatTime,
{
    fn format_event(
        &self, ctx: &FmtContext<'_, S, N>, mut writer: Writer<'_>, event: &Event<'_>,
    ) -> fmt::Result {
        #[cfg(feature = "tracing-log")]
        let normalized_meta = event.normalized_metadata();
        #[cfg(feature = "tracing-log")]
        let meta = normalized_meta.as_ref().unwrap_or_else(|| event.metadata());
        #[cfg(not(feature = "tracing-log"))]
        let meta = event.metadata();

        if let Some(prefix) = &self.prefix_string {
            write!(writer, "{} ", prefix)?;
        }

        // if the `Format` struct *also* has an ANSI color configuration,
        // override the writer...the API for configuring ANSI color codes on the
        // `Format` struct is deprecated, but we still need to honor those
        // configurations.
        // if let Some(ansi) = self.ansi {
        //     writer = writer.with_ansi(ansi);
        // }

        self.format_timestamp(&mut writer)?;

        if self.display_level {
            let fmt_level = {
                #[cfg(feature = "ansi")]
                {
                    FmtLevel::new(meta.level(), writer.has_ansi_escapes())
                }
                #[cfg(not(feature = "ansi"))]
                {
                    FmtLevel::new(meta.level())
                }
            };
            write!(writer, "{} ", fmt_level)?;
        }

        // Display request context (request_id, correlation_id) from span extensions or task-local
        if self.display_request_context {
            // Try span extensions first
            let mut request_ctx = None;
            if let Some(scope) = ctx.event_scope() {
                for span in scope.from_root() {
                    let ext = span.extensions();
                    if let Some(ctx) = ext.get::<RequestContext>()
                        && ctx.has_context()
                    {
                        request_ctx = Some(ctx.clone());
                        break;
                    }
                }
            }
            // Fall back to task-local storage
            if request_ctx.is_none() {
                request_ctx = super::context_layer::get_task_request_context();
            }

            if let Some(request_ctx) = request_ctx {
                let bracket_style = Style::new().dimmed();

                write!(writer, "{}", bracket_style.paint("["))?;
                let mut first = true;
                if let Some(ref req_id) = request_ctx.request_id {
                    #[cfg(feature = "ansi")]
                    {
                        if writer.has_ansi_escapes() {
                            write!(writer, "req:{}", Color::Cyan.paint(req_id))?;
                        } else {
                            write!(writer, "req:{}", req_id)?;
                        }
                    }
                    #[cfg(not(feature = "ansi"))]
                    write!(writer, "req:{}", req_id)?;
                    first = false;
                }
                if let Some(ref corr_id) = request_ctx.correlation_id {
                    // Only show correlation_id if different from request_id
                    if request_ctx.request_id.as_ref() != Some(corr_id) {
                        if !first {
                            writer.write_char(' ')?;
                        }
                        #[cfg(feature = "ansi")]
                        {
                            if writer.has_ansi_escapes() {
                                write!(writer, "corr:{}", Color::Cyan.paint(corr_id))?;
                            } else {
                                write!(writer, "corr:{}", corr_id)?;
                            }
                        }
                        #[cfg(not(feature = "ansi"))]
                        write!(writer, "corr:{}", corr_id)?;
                    }
                }
                write!(writer, "{} ", bracket_style.paint("]"))?;
            }
        }

        if self.display_thread_name {
            let current_thread = std::thread::current();
            match current_thread.name() {
                Some(name) => {
                    write!(writer, "{} ", FmtThreadName::new(name))?;
                }
                // fall-back to thread id when name is absent and ids are not enabled
                None if !self.display_thread_id => {
                    write!(writer, "{:0>2?} ", current_thread.id())?;
                }
                _ => {}
            }
        }

        if self.display_thread_id {
            write!(writer, "{:0>2?} ", std::thread::current().id())?;
        }

        // let dimmed = writer.dimmed();
        let dimmed = Style::new().dimmed();

        if self.display_event_scope
            && let Some(scope) = ctx.event_scope()
        {
            // let bold = writer.bold();
            let bold = Style::new().bold();

            let mut seen = false;

            for span in scope.from_root() {
                write!(writer, "{}", bold.paint(span.metadata().name()))?;
                seen = true;

                let ext = span.extensions();
                if let Some(fields) = &ext.get::<FormattedFields<N>>()
                    && !fields.is_empty()
                {
                    write!(writer, "{}{}{}", bold.paint("{"), fields, bold.paint("}"))?;
                }
                write!(writer, "{}", dimmed.paint(":"))?;
            }

            if seen {
                writer.write_char(' ')?;
            }
        };

        if self.display_target {
            write!(writer, "{}{} ", dimmed.paint(meta.target()), dimmed.paint(":"))?;
        }

        let line_number = if self.display_line_number { meta.line() } else { None };

        if self.display_filename
            && let Some(filename) = meta.file()
        {
            write!(
                writer,
                "{}{}{}",
                dimmed.paint(filename),
                dimmed.paint(":"),
                if line_number.is_some() { "" } else { " " }
            )?;
        }

        if let Some(line_number) = line_number {
            write!(writer, "{}{}:{} ", dimmed.prefix(), line_number, dimmed.suffix())?;
        }

        ctx.format_fields(writer.by_ref(), event)?;
        writeln!(writer)
    }
}

struct FmtThreadName<'a> {
    name: &'a str,
}

impl<'a> FmtThreadName<'a> {
    pub(crate) fn new(name: &'a str) -> Self {
        Self { name }
    }
}

impl<'a> fmt::Display for FmtThreadName<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use std::sync::atomic::{
            AtomicUsize,
            Ordering::{AcqRel, Acquire, Relaxed},
        };

        // Track the longest thread name length we've seen so far in an atomic,
        // so that it can be updated by any thread.
        static MAX_LEN: AtomicUsize = AtomicUsize::new(0);
        let len = self.name.len();
        // Snapshot the current max thread name length.
        let mut max_len = MAX_LEN.load(Relaxed);

        while len > max_len {
            // Try to set a new max length, if it is still the value we took a
            // snapshot of.
            match MAX_LEN.compare_exchange(max_len, len, AcqRel, Acquire) {
                // We successfully set the new max value
                Ok(_) => break,
                // Another thread set a new max value since we last observed
                // it! It's possible that the new length is actually longer than
                // ours, so we'll loop again and check whether our length is
                // still the longest. If not, we'll just use the newer value.
                Err(actual) => max_len = actual,
            }
        }

        // pad thread name using `max_len`
        write!(f, "{:>width$}", self.name, width = max_len)
    }
}

struct FmtLevel<'a> {
    level: &'a Level,
    #[cfg(feature = "ansi")]
    ansi: bool,
}

impl<'a> FmtLevel<'a> {
    #[cfg(feature = "ansi")]
    pub(crate) fn new(level: &'a Level, ansi: bool) -> Self {
        Self { level, ansi }
    }

    #[cfg(not(feature = "ansi"))]
    pub(crate) fn new(level: &'a Level) -> Self {
        Self { level }
    }
}

const TRACE_STR: &str = "TRACE";
const DEBUG_STR: &str = "DEBUG";
const INFO_STR: &str = " INFO";
const WARN_STR: &str = " WARN";
const ERROR_STR: &str = "ERROR";

#[cfg(not(feature = "ansi"))]
impl<'a> fmt::Display for FmtLevel<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self.level {
            Level::TRACE => f.pad(TRACE_STR),
            Level::DEBUG => f.pad(DEBUG_STR),
            Level::INFO => f.pad(INFO_STR),
            Level::WARN => f.pad(WARN_STR),
            Level::ERROR => f.pad(ERROR_STR),
        }
    }
}

#[cfg(feature = "ansi")]
impl<'a> fmt::Display for FmtLevel<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.ansi {
            match *self.level {
                Level::TRACE => write!(f, "{}", Color::Purple.paint(TRACE_STR)),
                Level::DEBUG => write!(f, "{}", Color::Blue.paint(DEBUG_STR)),
                Level::INFO => write!(f, "{}", Color::Green.paint(INFO_STR)),
                Level::WARN => write!(f, "{}", Color::Yellow.paint(WARN_STR)),
                Level::ERROR => write!(f, "{}", Color::Red.paint(ERROR_STR)),
            }
        } else {
            match *self.level {
                Level::TRACE => f.pad(TRACE_STR),
                Level::DEBUG => f.pad(DEBUG_STR),
                Level::INFO => f.pad(INFO_STR),
                Level::WARN => f.pad(WARN_STR),
                Level::ERROR => f.pad(ERROR_STR),
            }
        }
    }
}
