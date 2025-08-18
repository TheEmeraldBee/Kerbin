use serde::{
    Deserialize,
    de::{self, Visitor},
};
use std::{error::Error as StdError, fmt};

#[derive(Debug, PartialEq)]
pub struct DeserializerError(String);

impl fmt::Display for DeserializerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl StdError for DeserializerError {}

impl de::Error for DeserializerError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        DeserializerError(msg.to_string())
    }
}

pub struct Deserializer<'de> {
    input: &'de [String],
    index: usize,
}

impl<'de> Deserializer<'de> {
    pub fn from_slice(input: &'de [String]) -> Self {
        Deserializer { input, index: 0 }
    }

    fn next_value(&mut self) -> Result<&'de str, DeserializerError> {
        let value = self
            .input
            .get(self.index)
            .ok_or_else(|| de::Error::custom("Unexpected end of input slice"))?;
        self.index += 1;
        Ok(value)
    }
}

pub fn from_slice<'a, T>(s: &'a [String]) -> Result<T, DeserializerError>
where
    T: Deserialize<'a>,
{
    let mut deserializer = Deserializer::from_slice(s);
    T::deserialize(&mut deserializer)
}

impl<'de> de::Deserializer<'de> for &mut Deserializer<'de> {
    type Error = DeserializerError;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_seq(self)
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_enum(self)
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_borrowed_str(self.next_value()?)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_string(self.next_value()?.to_owned())
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self.next_value()?;
        let parsed = value.parse().map_err(de::Error::custom)?;
        visitor.visit_bool(parsed)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self.next_value()?;
        let parsed = value.parse().map_err(de::Error::custom)?;
        visitor.visit_u64(parsed)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self.next_value()?;
        let parsed = value.parse().map_err(de::Error::custom)?;
        visitor.visit_i64(parsed)
    }

    serde::forward_to_deserialize_any! {
        u8 u16 u32 u128 i8 i16 i32 i128 f32 f64 char bytes
        byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map identifier ignored_any
    }
}

impl<'de> de::SeqAccess<'de> for &mut Deserializer<'de> {
    type Error = DeserializerError;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        if self.index >= self.input.len() {
            return Ok(None);
        }
        seed.deserialize(&mut **self).map(Some)
    }
}

impl<'de> de::EnumAccess<'de> for &mut Deserializer<'de> {
    type Error = DeserializerError;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let variant = seed.deserialize(&mut *self)?;
        Ok((variant, self))
    }
}

impl<'de> de::VariantAccess<'de> for &mut Deserializer<'de> {
    type Error = DeserializerError;

    fn unit_variant(self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        seed.deserialize(self)
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_seq(self)
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_seq(self)
    }
}
