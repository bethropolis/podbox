//! Enumerate every dotted config key path from serde's derive metadata.
//!
//! `toml::Value::try_from(&Config)` omits fields whose value equals a
//! default, so a key cannot be validated by looking it up in a serialized
//! config. Instead, run `Config::deserialize` against a deserializer that
//! reports the field names it is asked for and supplies inert values. The
//! list stays in sync with the schema automatically: serde's derive passes
//! `FIELDS` to `deserialize_struct`.
//!
//! Pure module: no host dependencies, compiles for wasm.

use std::cell::RefCell;
use std::collections::BTreeSet;

use serde::Deserialize;
use serde::de::value::{Error, StrDeserializer};
use serde::de::{
    DeserializeSeed, Deserializer, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor,
};

use super::Config;

/// Every dotted key path the config schema accepts, including nested tables
/// and array entries (`security.secrets`, `provision`, …).
pub fn config_key_paths() -> BTreeSet<String> {
    let keys = RefCell::new(BTreeSet::new());
    // Untagged enums accept either a bool or a string shorthand depending on
    // the field, and a mismatched value aborts the walk. Run one pass per
    // shorthand and union what each discovered.
    for any in [AnyValue::Bool, AnyValue::String] {
        let _ = Config::deserialize(Collector {
            path: String::new(),
            keys: &keys,
            any,
        });
    }
    keys.into_inner()
}

#[derive(Debug, Clone, Copy)]
enum AnyValue {
    Bool,
    String,
}

/// A deserializer that records struct field paths and hands back inert
/// values so `Config::deserialize` can walk the whole schema.
struct Collector<'a> {
    path: String,
    keys: &'a RefCell<BTreeSet<String>>,
    any: AnyValue,
}

impl Collector<'_> {
    fn child(&self, segment: &str) -> Self {
        let path = if segment.is_empty() {
            self.path.clone()
        } else if self.path.is_empty() {
            segment.to_owned()
        } else {
            format!("{}.{segment}", self.path)
        };
        Self {
            path,
            keys: self.keys,
            any: self.any,
        }
    }
}

/// A map access over serde's `FIELDS` array.
struct FieldMap<'a, 'de> {
    fields: &'de [&'de str],
    index: usize,
    collector: Collector<'a>,
}

impl<'de> MapAccess<'de> for FieldMap<'_, 'de> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: DeserializeSeed<'de>,
    {
        let Some(field) = self.fields.get(self.index) else {
            return Ok(None);
        };
        self.index += 1;
        seed.deserialize(StrDeserializer::new(field)).map(Some)
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: DeserializeSeed<'de>,
    {
        seed.deserialize(self.collector.child(self.fields[self.index - 1]))
    }
}

struct OneElement<'a> {
    collector: Collector<'a>,
    done: bool,
}

impl<'de> SeqAccess<'de> for OneElement<'_> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        if self.done {
            return Ok(None);
        }
        self.done = true;
        seed.deserialize(self.collector.child("")).map(Some)
    }
}

struct OneEntry<'a, 'de> {
    key: &'de str,
    collector: Collector<'a>,
    done: bool,
}

impl<'de> MapAccess<'de> for OneEntry<'_, 'de> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: DeserializeSeed<'de>,
    {
        if self.done {
            return Ok(None);
        }
        seed.deserialize(StrDeserializer::new(self.key)).map(Some)
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: DeserializeSeed<'de>,
    {
        self.done = true;
        let child = self.collector.child(self.key);
        seed.deserialize(child)
    }
}

struct EnumOne<'a, 'de> {
    variant: &'de str,
    collector: Collector<'a>,
}

impl<'de> EnumAccess<'de> for EnumOne<'_, 'de> {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: DeserializeSeed<'de>,
    {
        let variant = seed.deserialize(StrDeserializer::new(self.variant))?;
        Ok((variant, self))
    }
}

impl<'de> VariantAccess<'de> for EnumOne<'_, 'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        seed.deserialize(self.collector)
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_seq(OneElement {
            collector: self.collector,
            done: false,
        })
    }

    fn struct_variant<V>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_map(FieldMap {
            fields,
            index: 0,
            collector: self.collector,
        })
    }
}

impl<'de> Deserializer<'de> for Collector<'_> {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        // Untagged and hand-written enums ask for `any`. Pick the shorthand
        // the field at this path accepts, so the walk continues past it.
        let path = self.path.as_str();
        if path == "integration.gpu"
            || path.contains("host_exec")
            || path.contains("secrets")
            || path.contains("services")
        {
            return visitor.visit_str(if path == "integration.gpu" {
                "auto"
            } else {
                "x"
            });
        }
        if path.contains("xdg_open") || path.contains("xdg_dirs") {
            return visitor.visit_bool(false);
        }
        match self.any {
            AnyValue::Bool => visitor.visit_bool(false),
            AnyValue::String => visitor.visit_str("x"),
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_bool(false)
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i8(1)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i16(1)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i32(1)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i64(1)
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u8(1)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u16(1)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u32(1)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u64(1)
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_f32(1.0)
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_f64(1.0)
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_char('x')
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_str("x")
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_string("x".to_owned())
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_bytes(b"x")
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_byte_buf(b"x".to_vec())
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_some(self)
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }

    fn deserialize_unit_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_seq(OneElement {
            collector: self,
            done: false,
        })
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_map(OneEntry {
            key: "x",
            collector: self,
            done: false,
        })
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        for field in fields {
            let path = if self.path.is_empty() {
                (*field).to_owned()
            } else {
                format!("{}.{field}", self.path)
            };
            self.keys.borrow_mut().insert(path);
        }
        visitor.visit_map(FieldMap {
            fields,
            index: 0,
            collector: self,
        })
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let variant = variants.first().copied().unwrap_or_default();
        visitor.visit_enum(EnumOne {
            variant,
            collector: self,
        })
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_str("x")
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_nested_field_paths() {
        let paths = config_key_paths();
        for key in [
            "image.base",
            "container.shell",
            "integration.audio",
            "network.mode",
            "wayland.firewall",
            "security.secrets",
            "provision",
        ] {
            assert!(paths.contains(key), "missing key path '{key}'");
        }
        assert!(!paths.contains("bogus.key"));
        assert!(!paths.contains("container.bogus"));
    }
}
