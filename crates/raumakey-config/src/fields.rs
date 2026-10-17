//! Field-level decoding and extension re-emission shared by the TOML and JSON documents.

use core::ops::RangeInclusive;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde::ser::SerializeMap;

use crate::document::{FieldIssue, IssueReason};

/// A parsed value of a document format (TOML or JSON).
pub(crate) trait Node: Sized {
    /// The format's table/object type.
    type Map: Default;

    /// Removes `key` from `map`.
    fn take(map: &mut Self::Map, key: &str) -> Option<Self>;
    /// The value of `key` in `map`.
    fn get<'m>(map: &'m Self::Map, key: &str) -> Option<&'m Self>;
    /// Inserts `value` under `key`, replacing any previous value.
    fn insert(map: &mut Self::Map, key: &str, value: Self);
    /// The boolean, if this is a boolean.
    fn as_bool(&self) -> Option<bool>;
    /// Whether this is an integer (of any sign or size).
    fn is_integer(&self) -> bool;
    /// The value, if this is an integer that fits `u64`.
    fn as_u64(&self) -> Option<u64>;
    /// The string, if this is a string.
    fn as_str(&self) -> Option<&str>;
    /// The string, if this is a string.
    fn into_string(self) -> Option<String>;
    /// The table/object, or the value back.
    fn into_map(self) -> Result<Self::Map, Self>;
    /// The array, or the value back.
    fn into_array(self) -> Result<Vec<Self>, Self>;
    /// Decodes the value with serde.
    fn decode<T: DeserializeOwned>(self) -> Option<T>;
}

impl Node for toml::Value {
    type Map = toml::Table;

    fn take(map: &mut Self::Map, key: &str) -> Option<Self> {
        map.remove(key)
    }

    fn get<'m>(map: &'m Self::Map, key: &str) -> Option<&'m Self> {
        map.get(key)
    }

    fn insert(map: &mut Self::Map, key: &str, value: Self) {
        map.insert(key.to_owned(), value);
    }

    fn as_bool(&self) -> Option<bool> {
        toml::Value::as_bool(self)
    }

    fn is_integer(&self) -> bool {
        toml::Value::is_integer(self)
    }

    fn as_u64(&self) -> Option<u64> {
        self.as_integer()
            .and_then(|value| u64::try_from(value).ok())
    }

    fn as_str(&self) -> Option<&str> {
        toml::Value::as_str(self)
    }

    fn into_string(self) -> Option<String> {
        match self {
            Self::String(text) => Some(text),
            _ => None,
        }
    }

    fn into_map(self) -> Result<Self::Map, Self> {
        match self {
            Self::Table(table) => Ok(table),
            other => Err(other),
        }
    }

    fn into_array(self) -> Result<Vec<Self>, Self> {
        match self {
            Self::Array(items) => Ok(items),
            other => Err(other),
        }
    }

    fn decode<T: DeserializeOwned>(self) -> Option<T> {
        self.try_into().ok()
    }
}

impl Node for serde_json::Value {
    type Map = serde_json::Map<String, serde_json::Value>;

    fn take(map: &mut Self::Map, key: &str) -> Option<Self> {
        map.remove(key)
    }

    fn get<'m>(map: &'m Self::Map, key: &str) -> Option<&'m Self> {
        map.get(key)
    }

    fn insert(map: &mut Self::Map, key: &str, value: Self) {
        map.insert(key.to_owned(), value);
    }

    fn as_bool(&self) -> Option<bool> {
        serde_json::Value::as_bool(self)
    }

    fn is_integer(&self) -> bool {
        self.is_i64() || self.is_u64()
    }

    fn as_u64(&self) -> Option<u64> {
        serde_json::Value::as_u64(self)
    }

    fn as_str(&self) -> Option<&str> {
        serde_json::Value::as_str(self)
    }

    fn into_string(self) -> Option<String> {
        match self {
            Self::String(text) => Some(text),
            _ => None,
        }
    }

    fn into_map(self) -> Result<Self::Map, Self> {
        match self {
            Self::Object(map) => Ok(map),
            other => Err(other),
        }
    }

    fn into_array(self) -> Result<Vec<Self>, Self> {
        match self {
            Self::Array(items) => Ok(items),
            other => Err(other),
        }
    }

    fn decode<T: DeserializeOwned>(self) -> Option<T> {
        serde_json::from_value(self).ok()
    }
}

/// Reads a boolean.
pub(crate) fn boolean<N: Node>(value: &N) -> Result<bool, IssueReason> {
    value.as_bool().ok_or(IssueReason::InvalidType)
}

/// Reads an unsigned integer restricted to `range`.
pub(crate) fn integer_in<N: Node>(
    value: &N,
    range: RangeInclusive<u64>,
) -> Result<u64, IssueReason> {
    if !value.is_integer() {
        return Err(IssueReason::InvalidType);
    }
    value
        .as_u64()
        .filter(|number| range.contains(number))
        .ok_or(IssueReason::OutOfRange)
}

/// Reads a `u32`.
pub(crate) fn integer_u32<N: Node>(value: &N) -> Result<u32, IssueReason> {
    let wide = integer_in(value, 0..=u64::from(u32::MAX))?;
    u32::try_from(wide).map_err(|_| IssueReason::OutOfRange)
}

/// Reads a string.
pub(crate) fn text<N: Node>(value: &N) -> Result<&str, IssueReason> {
    value.as_str().ok_or(IssueReason::InvalidType)
}

/// Reads a string, taking ownership.
pub(crate) fn owned_text<N: Node>(value: N) -> Result<String, IssueReason> {
    value.into_string().ok_or(IssueReason::InvalidType)
}

/// Reads a string-encoded enum through its serde names.
pub(crate) fn keyword<N: Node, T: DeserializeOwned>(value: N) -> Result<T, IssueReason> {
    if value.as_str().is_none() {
        return Err(IssueReason::InvalidType);
    }
    value.decode().ok_or(IssueReason::InvalidValue)
}

/// Decodes the known fields of one table independently.
///
/// Every consuming accessor removes its key, so whatever remains at the end is the table's
/// unknown keys. Problems are recorded as [`FieldIssue`]s with schema paths only.
pub(crate) struct Fields<'i, N: Node> {
    map: N::Map,
    prefix: String,
    issues: &'i mut Vec<FieldIssue>,
}

impl<'i, N: Node> Fields<'i, N> {
    /// Starts decoding `map`, whose fields are reported under `prefix`.
    pub(crate) fn new(map: N::Map, prefix: &str, issues: &'i mut Vec<FieldIssue>) -> Self {
        Self {
            map,
            prefix: prefix.to_owned(),
            issues,
        }
    }

    /// Decodes a nested table reported under `<prefix>.<name>`.
    pub(crate) fn child(&mut self, map: N::Map, name: &str) -> Fields<'_, N> {
        let prefix = self.path(name);
        self.at(map, prefix)
    }

    /// Decodes a nested table reported under the full path `path`, such as `macro[3]`.
    pub(crate) fn at(&mut self, map: N::Map, path: String) -> Fields<'_, N> {
        Fields {
            map,
            prefix: path,
            issues: &mut *self.issues,
        }
    }

    /// Schema path of `key` inside this table.
    pub(crate) fn path(&self, key: &str) -> String {
        if self.prefix.is_empty() {
            key.to_owned()
        } else {
            format!("{}.{key}", self.prefix)
        }
    }

    /// Schema path of element `index` of the array `key`.
    pub(crate) fn element_path(&self, key: &str, index: usize) -> String {
        format!("{}[{index}]", self.path(key))
    }

    /// Records an issue at a full path.
    pub(crate) fn report_path(&mut self, path: String, reason: IssueReason) {
        self.issues.push(FieldIssue { path, reason });
    }

    /// Records an issue for `key`.
    pub(crate) fn report(&mut self, key: &str, reason: IssueReason) {
        let path = self.path(key);
        self.report_path(path, reason);
    }

    /// Removes and returns the raw value of `key`.
    pub(crate) fn take(&mut self, key: &str) -> Option<N> {
        N::take(&mut self.map, key)
    }

    /// The raw value of `key`, left in place.
    pub(crate) fn peek(&self, key: &str) -> Option<&N> {
        N::get(&self.map, key)
    }

    /// Stores `value` under `key` in the rest returned by [`Fields::into_rest`], such as the
    /// unknown keys of a nested table under that table's (known) name.
    pub(crate) fn keep(&mut self, key: &str, value: N) {
        N::insert(&mut self.map, key, value);
    }

    /// Removes keys whose values were already read through [`Peek`].
    pub(crate) fn drop_keys(&mut self, keys: &[&str]) {
        for key in keys {
            let _ = self.take(key);
        }
    }

    /// A read-only view for identity checks: nothing can be consumed through it.
    pub(crate) fn peeker(&mut self) -> Peek<'_, 'i, N> {
        Peek { fields: self }
    }

    /// Takes `key` and converts it: `None` when absent, `Some(None)` when invalid (reported).
    fn convert<T>(
        &mut self,
        key: &str,
        convert: impl FnOnce(N) -> Result<T, IssueReason>,
    ) -> Option<Option<T>> {
        let value = self.take(key)?;
        match convert(value) {
            Ok(converted) => Some(Some(converted)),
            Err(reason) => {
                self.report(key, reason);
                Some(None)
            }
        }
    }

    /// A field with a default: absent takes `default` silently, invalid takes it and is
    /// reported.
    pub(crate) fn field<T>(
        &mut self,
        key: &str,
        default: T,
        convert: impl FnOnce(N) -> Result<T, IssueReason>,
    ) -> T {
        self.convert(key, convert).flatten().unwrap_or(default)
    }

    /// A field the schema requires but whose absence does not invalidate its table: absent is
    /// reported as [`IssueReason::Missing`], invalid with its reason, and both take `default`.
    pub(crate) fn needed<T>(
        &mut self,
        key: &str,
        default: T,
        convert: impl FnOnce(N) -> Result<T, IssueReason>,
    ) -> T {
        match self.convert(key, convert) {
            None => {
                self.report(key, IssueReason::Missing);
                default
            }
            Some(converted) => converted.unwrap_or(default),
        }
    }

    /// An optional field: `None` when absent or invalid (reported).
    pub(crate) fn optional<T>(
        &mut self,
        key: &str,
        convert: impl FnOnce(N) -> Result<T, IssueReason>,
    ) -> Option<T> {
        self.convert(key, convert).flatten()
    }

    /// A boolean field.
    pub(crate) fn bool(&mut self, key: &str, default: bool) -> bool {
        self.field(key, default, |value| boolean(&value))
    }

    /// An unsigned integer field.
    pub(crate) fn u64(&mut self, key: &str, default: u64) -> u64 {
        self.u64_in(key, default, 0..=u64::MAX)
    }

    /// An unsigned integer field restricted to `range`.
    pub(crate) fn u64_in(&mut self, key: &str, default: u64, range: RangeInclusive<u64>) -> u64 {
        self.field(key, default, |value| integer_in(&value, range))
    }

    /// A `u32` field restricted to `range`.
    pub(crate) fn u32_in(&mut self, key: &str, default: u32, range: RangeInclusive<u32>) -> u32 {
        let wide = self.u64_in(
            key,
            u64::from(default),
            u64::from(*range.start())..=u64::from(*range.end()),
        );
        u32::try_from(wide).unwrap_or(default)
    }

    /// A string field.
    pub(crate) fn string(&mut self, key: &str, default: &str) -> String {
        self.field(key, default.to_owned(), |value| {
            value.into_string().ok_or(IssueReason::InvalidType)
        })
    }

    /// An optional string field; an empty string reads as absent.
    pub(crate) fn optional_string(
        &mut self,
        key: &str,
        valid: impl Fn(&str) -> bool,
    ) -> Option<String> {
        self.optional(key, |value| match value.into_string() {
            None => Err(IssueReason::InvalidType),
            Some(text) if text.is_empty() || valid(&text) => Ok(text),
            Some(_) => Err(IssueReason::InvalidValue),
        })
        .filter(|text| !text.is_empty())
    }

    /// A keyword (string-encoded enum) field.
    pub(crate) fn keyword<T: DeserializeOwned>(&mut self, key: &str, default: T) -> T {
        self.field(key, default, keyword)
    }

    /// A nested table; `None` when absent or of the wrong type (reported).
    pub(crate) fn table(&mut self, key: &str) -> Option<N::Map> {
        self.optional(key, |value| {
            value.into_map().map_err(|_| IssueReason::InvalidType)
        })
    }

    /// An array; `None` when absent or of the wrong type (reported).
    pub(crate) fn array(&mut self, key: &str) -> Option<Vec<N>> {
        self.optional(key, |value| {
            value.into_array().map_err(|_| IssueReason::InvalidType)
        })
    }

    /// An array of strings parsed one by one.
    ///
    /// Bad elements are dropped and reported. When the array has elements but none of them is
    /// usable, the field takes `default` (matrix #45): only an explicitly empty array means
    /// "none".
    pub(crate) fn string_list<T>(
        &mut self,
        key: &str,
        default: Vec<T>,
        parse: impl Fn(&str) -> Option<T>,
    ) -> Vec<T> {
        let Some(items) = self.array(key) else {
            return default;
        };
        let mut parsed = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            let outcome = match item.as_str() {
                None => Err(IssueReason::InvalidType),
                Some(text) => parse(text).ok_or(IssueReason::InvalidValue),
            };
            match outcome {
                Ok(value) => parsed.push(value),
                Err(reason) => {
                    let path = self.element_path(key, index);
                    self.report_path(path, reason);
                }
            }
        }
        if parsed.is_empty() && !items.is_empty() {
            default
        } else {
            parsed
        }
    }

    /// Visits the tables of the array `key` in order, each with its element path; elements
    /// that are not tables are dropped and reported.
    pub(crate) fn each_table(
        &mut self,
        key: &str,
        mut visit: impl FnMut(&mut Self, String, N::Map),
    ) {
        let Some(items) = self.array(key) else {
            return;
        };
        for (index, item) in items.into_iter().enumerate() {
            let path = self.element_path(key, index);
            match item.into_map() {
                Ok(map) => visit(self, path, map),
                Err(_) => self.report_path(path, IssueReason::InvalidType),
            }
        }
    }

    /// The keys no accessor consumed.
    pub(crate) fn into_rest(self) -> N::Map {
        self.map
    }
}

/// Read-only access to an entry's fields, used to decide whether the entry can be decoded
/// before anything is consumed, so that a rejected entry is still complete.
pub(crate) struct Peek<'f, 'i, N: Node> {
    fields: &'f mut Fields<'i, N>,
}

impl<N: Node> Peek<'_, '_, N> {
    /// Reads a field the entry cannot exist without. Absence is reported as
    /// [`IssueReason::Missing`] and a bad value with the reason `convert` gives.
    pub(crate) fn require<T>(
        &mut self,
        key: &str,
        convert: impl FnOnce(&N) -> Result<T, IssueReason>,
    ) -> Option<T> {
        let outcome = match self.fields.peek(key) {
            None => Err(IssueReason::Missing),
            Some(value) => convert(value),
        };
        match outcome {
            Ok(value) => Some(value),
            Err(reason) => {
                self.fields.report(key, reason);
                None
            }
        }
    }

    /// The raw value of `key`.
    pub(crate) fn get(&self, key: &str) -> Option<&N> {
        self.fields.peek(key)
    }

    /// Records an issue for `key`.
    pub(crate) fn report(&mut self, key: &str, reason: IssueReason) {
        self.fields.report(key, reason);
    }
}

/// A table/object whose entries can be re-emitted.
pub(crate) trait ExtensionMap {
    /// Serializes every entry whose key is not in `known`.
    fn serialize_filtered<S: SerializeMap>(
        &self,
        map: &mut S,
        known: &[&str],
    ) -> Result<(), S::Error>;
}

impl ExtensionMap for toml::Table {
    fn serialize_filtered<S: SerializeMap>(
        &self,
        map: &mut S,
        known: &[&str],
    ) -> Result<(), S::Error> {
        for (key, value) in self {
            if !known.contains(&key.as_str()) {
                map.serialize_entry(key, value)?;
            }
        }
        Ok(())
    }
}

impl ExtensionMap for serde_json::Map<String, serde_json::Value> {
    fn serialize_filtered<S: SerializeMap>(
        &self,
        map: &mut S,
        known: &[&str],
    ) -> Result<(), S::Error> {
        for (key, value) in self {
            if !known.contains(&key.as_str()) {
                map.serialize_entry(key, value)?;
            }
        }
        Ok(())
    }
}

/// Unknown keys to flatten into a wire struct, minus any key the schema already writes.
pub(crate) struct Extra<'a, M> {
    map: Option<&'a M>,
    known: &'static [&'static str],
}

impl<'a, M> Extra<'a, M> {
    /// Re-emits `map` except the keys in `known`.
    pub(crate) fn new(map: Option<&'a M>, known: &'static [&'static str]) -> Self {
        Self { map, known }
    }
}

impl<M: ExtensionMap> Serialize for Extra<'_, M> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        if let Some(extensions) = self.map {
            extensions.serialize_filtered(&mut map, self.known)?;
        }
        map.end()
    }
}
