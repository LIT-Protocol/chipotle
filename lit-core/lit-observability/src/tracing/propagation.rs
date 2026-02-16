//! This module contains wrapping types for injecting and extracting tracing propagation context.

use std::collections::HashMap;

use opentelemetry::propagation::{Extractor, Injector};

/// MetadataMap is a wrapping type for injecting and extracting tracing propagation context into and from a `tonic::metadata::MetadataMap`.
pub struct TonicMetadataMap<'a>(pub &'a mut tonic::metadata::MetadataMap);

impl<'a> Injector for TonicMetadataMap<'a> {
    /// Set a key and value in the MetadataMap.  Does nothing if the key or value are not valid inputs
    fn set(&mut self, key: &str, value: String) {
        if let Ok(key) = tonic::metadata::MetadataKey::from_bytes(key.as_bytes()) {
            if let Ok(val) = tonic::metadata::MetadataValue::try_from(&value) {
                self.0.insert(key, val);
            }
        }
    }
}

impl<'a> Extractor for TonicMetadataMap<'a> {
    /// Get a value for a key from the MetadataMap.  If the value can't be converted to &str, returns None
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|metadata| metadata.to_str().ok())
    }

    /// Collect all the keys from the MetadataMap.
    fn keys(&self) -> Vec<&str> {
        self.0
            .keys()
            .map(|key| match key {
                tonic::metadata::KeyRef::Ascii(v) => v.as_str(),
                tonic::metadata::KeyRef::Binary(v) => v.as_str(),
            })
            .collect::<Vec<_>>()
    }
}

/// HttpMetadataMap is a wrapping type for injecting and extracting tracing propagation context into and from a `tonic::codegen::http::HeaderMap`.
pub struct HttpMetadataMap<'a>(pub &'a mut tonic::codegen::http::HeaderMap);

impl<'a> Extractor for HttpMetadataMap<'a> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|header_value| header_value.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|key| key.as_str()).collect()
    }
}

pub struct HashMapMetadataMap<'a>(pub &'a mut HashMap<String, String>);

impl<'a> Injector for HashMapMetadataMap<'a> {
    fn set(&mut self, key: &str, value: String) {
        self.0.insert(key.to_lowercase(), value);
    }
}

impl<'a> Extractor for HashMapMetadataMap<'a> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(|v| v.as_str())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|k| k.as_str()).collect()
    }
}
