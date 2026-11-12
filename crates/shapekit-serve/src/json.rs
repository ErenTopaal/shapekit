//! JSON text written straight from a `Serialize` value into a byte buffer, byte for byte
//! what `serde_json::to_value(v)?.to_string()` gives: compact, object keys in sorted order
//! (serde_json's map is a B-tree without `preserve_order`), a repeated key keeping its last
//! value.
//!
//! a `Value` tree of XDR-JSON costs 17 to 35 resident bytes per byte of text, nearly all of
//! it B-tree nodes for small objects, and the allocator keeps that high-water mark per
//! thread. here an object's members are written in place as they come; only an object whose
//! members arrive out of order is rewritten, which briefly holds that one object twice.

use std::borrow::Cow;
use std::ops::Range;

use serde::ser::{self, Error as _, Serialize, Serializer as _};
use serde_json::{Error, Value};

/// Appends the JSON text of `v` to `out`.
pub fn write<T: Serialize + ?Sized>(out: &mut Vec<u8>, v: &T) -> Result<(), Error> {
    v.serialize(Ser { out })
}

/// `s` as a JSON string.
fn write_str(out: &mut Vec<u8>, s: &str) {
    // writing into a vec cannot fail, and a str always serialises
    serde_json::to_writer(out, s).expect("a string serialises into a vec");
}

/// Scalars go through serde_json's own value serializer, so numbers, strings, bytes and
/// unit variants come out exactly as a value tree would print them.
fn scalar(out: &mut Vec<u8>, v: Result<Value, Error>) -> Result<(), Error> {
    serde_json::to_writer(out, &v?)
}

/// The text a map key becomes, by the rules of serde_json's value serializer: strings as
/// they are, integers and the like as their decimal text, anything else an error.
fn map_key<T: Serialize + ?Sized>(key: &T) -> Result<String, Error> {
    use ser::SerializeMap;
    let mut m = serde_json::value::Serializer.serialize_map(Some(1))?;
    m.serialize_key(key)?;
    m.serialize_value(&())?;
    match m.end()? {
        Value::Object(o) => o
            .into_iter()
            .next()
            .map(|(k, _)| k)
            .ok_or_else(|| Error::custom("a map key produced no entry")),
        _ => Err(Error::custom(
            "a map serialised to something other than an object",
        )),
    }
}

/// One JSON object being written. Members go straight into the buffer; `end` puts them in
/// key order if they did not arrive that way.
pub struct Object<'a> {
    out: &'a mut Vec<u8>,
    start: usize,
    members: Vec<(Cow<'static, str>, Range<usize>)>,
}

impl<'a> Object<'a> {
    pub fn new(out: &'a mut Vec<u8>) -> Self {
        out.push(b'{');
        let start = out.len();
        Object {
            out,
            start,
            members: Vec::new(),
        }
    }

    /// Writes one member whose value `f` appends to the buffer.
    pub fn member_with<E>(
        &mut self,
        key: impl Into<Cow<'static, str>>,
        f: impl FnOnce(&mut Vec<u8>) -> Result<(), E>,
    ) -> Result<(), E> {
        let key = key.into();
        if !self.members.is_empty() {
            self.out.push(b',');
        }
        let at = self.out.len();
        write_str(self.out, &key);
        self.out.push(b':');
        f(self.out)?;
        self.members.push((key, at..self.out.len()));
        Ok(())
    }

    pub fn member<T: Serialize + ?Sized>(
        &mut self,
        key: impl Into<Cow<'static, str>>,
        v: &T,
    ) -> Result<(), Error> {
        self.member_with(key, |out| write(out, v))
    }

    /// Closes the object and hands the buffer back.
    pub fn end(self) -> &'a mut Vec<u8> {
        let Object {
            out,
            start,
            members,
        } = self;
        if !members.windows(2).all(|w| w[0].0 < w[1].0) {
            let body = out.split_off(start);
            let mut order: Vec<usize> = (0..members.len()).collect();
            // stable, so of equal keys the one written last stays last
            order.sort_by(|&a, &b| members[a].0.cmp(&members[b].0));
            let mut first = true;
            for (n, &i) in order.iter().enumerate() {
                if order
                    .get(n + 1)
                    .is_some_and(|&j| members[j].0 == members[i].0)
                {
                    continue;
                }
                if !first {
                    out.push(b',');
                }
                first = false;
                let r = &members[i].1;
                out.extend_from_slice(&body[r.start - start..r.end - start]);
            }
        }
        out.push(b'}');
        out
    }
}

struct Ser<'a> {
    out: &'a mut Vec<u8>,
}

/// A JSON array, closed by `close`: `]`, or `]}` for a tuple variant.
pub struct Seq<'a> {
    out: &'a mut Vec<u8>,
    first: bool,
    close: &'static [u8],
}

impl Seq<'_> {
    fn element<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
        if !self.first {
            self.out.push(b',');
        }
        self.first = false;
        write(self.out, v)
    }

    fn close(self) {
        self.out.extend_from_slice(self.close);
    }
}

/// A map, struct or struct variant; a struct variant needs one more `}` for its wrapper.
pub struct Map<'a> {
    obj: Object<'a>,
    key: Option<String>,
    variant: bool,
}

impl Map<'_> {
    fn close(self) {
        let out = self.obj.end();
        if self.variant {
            out.push(b'}');
        }
    }
}

/// `{"<variant>":` for the externally tagged enum shapes.
fn open_variant(out: &mut Vec<u8>, variant: &str) {
    out.push(b'{');
    write_str(out, variant);
    out.push(b':');
}

impl<'a> ser::Serializer for Ser<'a> {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Seq<'a>;
    type SerializeTuple = Seq<'a>;
    type SerializeTupleStruct = Seq<'a>;
    type SerializeTupleVariant = Seq<'a>;
    type SerializeMap = Map<'a>;
    type SerializeStruct = Map<'a>;
    type SerializeStructVariant = Map<'a>;

    fn serialize_bool(self, v: bool) -> Result<(), Error> {
        scalar(self.out, serde_json::value::Serializer.serialize_bool(v))
    }
    fn serialize_i8(self, v: i8) -> Result<(), Error> {
        scalar(self.out, serde_json::value::Serializer.serialize_i8(v))
    }
    fn serialize_i16(self, v: i16) -> Result<(), Error> {
        scalar(self.out, serde_json::value::Serializer.serialize_i16(v))
    }
    fn serialize_i32(self, v: i32) -> Result<(), Error> {
        scalar(self.out, serde_json::value::Serializer.serialize_i32(v))
    }
    fn serialize_i64(self, v: i64) -> Result<(), Error> {
        scalar(self.out, serde_json::value::Serializer.serialize_i64(v))
    }
    fn serialize_i128(self, v: i128) -> Result<(), Error> {
        scalar(self.out, serde_json::value::Serializer.serialize_i128(v))
    }
    fn serialize_u8(self, v: u8) -> Result<(), Error> {
        scalar(self.out, serde_json::value::Serializer.serialize_u8(v))
    }
    fn serialize_u16(self, v: u16) -> Result<(), Error> {
        scalar(self.out, serde_json::value::Serializer.serialize_u16(v))
    }
    fn serialize_u32(self, v: u32) -> Result<(), Error> {
        scalar(self.out, serde_json::value::Serializer.serialize_u32(v))
    }
    fn serialize_u64(self, v: u64) -> Result<(), Error> {
        scalar(self.out, serde_json::value::Serializer.serialize_u64(v))
    }
    fn serialize_u128(self, v: u128) -> Result<(), Error> {
        scalar(self.out, serde_json::value::Serializer.serialize_u128(v))
    }
    fn serialize_f32(self, v: f32) -> Result<(), Error> {
        scalar(self.out, serde_json::value::Serializer.serialize_f32(v))
    }
    fn serialize_f64(self, v: f64) -> Result<(), Error> {
        scalar(self.out, serde_json::value::Serializer.serialize_f64(v))
    }
    fn serialize_char(self, v: char) -> Result<(), Error> {
        scalar(self.out, serde_json::value::Serializer.serialize_char(v))
    }
    fn serialize_str(self, v: &str) -> Result<(), Error> {
        write_str(self.out, v);
        Ok(())
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<(), Error> {
        scalar(self.out, serde_json::value::Serializer.serialize_bytes(v))
    }
    fn serialize_none(self) -> Result<(), Error> {
        self.serialize_unit()
    }
    fn serialize_some<T: Serialize + ?Sized>(self, v: &T) -> Result<(), Error> {
        v.serialize(self)
    }
    fn serialize_unit(self) -> Result<(), Error> {
        self.out.extend_from_slice(b"null");
        Ok(())
    }
    fn serialize_unit_struct(self, _name: &'static str) -> Result<(), Error> {
        self.serialize_unit()
    }
    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
    ) -> Result<(), Error> {
        self.serialize_str(variant)
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        v: &T,
    ) -> Result<(), Error> {
        v.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        v: &T,
    ) -> Result<(), Error> {
        open_variant(self.out, variant);
        write(self.out, v)?;
        self.out.push(b'}');
        Ok(())
    }
    fn serialize_seq(self, _len: Option<usize>) -> Result<Seq<'a>, Error> {
        self.out.push(b'[');
        Ok(Seq {
            out: self.out,
            first: true,
            close: b"]",
        })
    }
    fn serialize_tuple(self, len: usize) -> Result<Seq<'a>, Error> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_struct(self, _name: &'static str, len: usize) -> Result<Seq<'a>, Error> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Seq<'a>, Error> {
        open_variant(self.out, variant);
        self.out.push(b'[');
        Ok(Seq {
            out: self.out,
            first: true,
            close: b"]}",
        })
    }
    fn serialize_map(self, _len: Option<usize>) -> Result<Map<'a>, Error> {
        Ok(Map {
            obj: Object::new(self.out),
            key: None,
            variant: false,
        })
    }
    fn serialize_struct(self, _name: &'static str, len: usize) -> Result<Map<'a>, Error> {
        self.serialize_map(Some(len))
    }
    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Map<'a>, Error> {
        open_variant(self.out, variant);
        Ok(Map {
            obj: Object::new(self.out),
            key: None,
            variant: true,
        })
    }
}

impl ser::SerializeSeq for Seq<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
        self.element(v)
    }
    fn end(self) -> Result<(), Error> {
        self.close();
        Ok(())
    }
}

impl ser::SerializeTuple for Seq<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
        self.element(v)
    }
    fn end(self) -> Result<(), Error> {
        self.close();
        Ok(())
    }
}

impl ser::SerializeTupleStruct for Seq<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
        self.element(v)
    }
    fn end(self) -> Result<(), Error> {
        self.close();
        Ok(())
    }
}

impl ser::SerializeTupleVariant for Seq<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
        self.element(v)
    }
    fn end(self) -> Result<(), Error> {
        self.close();
        Ok(())
    }
}

impl ser::SerializeMap for Map<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), Error> {
        self.key = Some(map_key(key)?);
        Ok(())
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
        let key = self
            .key
            .take()
            .ok_or_else(|| Error::custom("a map value came without its key"))?;
        self.obj.member(key, v)
    }
    fn end(self) -> Result<(), Error> {
        self.close();
        Ok(())
    }
}

impl ser::SerializeStruct for Map<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        v: &T,
    ) -> Result<(), Error> {
        self.obj.member(key, v)
    }
    fn end(self) -> Result<(), Error> {
        self.close();
        Ok(())
    }
}

impl ser::SerializeStructVariant for Map<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        v: &T,
    ) -> Result<(), Error> {
        self.obj.member(key, v)
    }
    fn end(self) -> Result<(), Error> {
        self.close();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde::Serialize;
    use serde_json::json;
    use std::collections::BTreeMap;

    fn same<T: Serialize>(v: &T) {
        let mut out = Vec::new();
        super::write(&mut out, v).unwrap();
        let want = serde_json::to_value(v).unwrap().to_string();
        assert_eq!(String::from_utf8(out).unwrap(), want);
    }

    #[derive(Serialize)]
    enum E {
        Unit,
        New(u8),
        Tuple(u8, String),
        Struct { zeta: u8, alpha: Option<u8> },
    }

    #[derive(Serialize)]
    struct S {
        zz: Vec<E>,
        b: bytes::Bytes,
        #[serde(rename = "a\"quoted\u{1}")]
        q: (),
        m: BTreeMap<u32, &'static str>,
        n: i64,
        f: f64,
        nan: f64,
        c: char,
        aa: Option<String>,
    }

    mod bytes {
        #[derive(serde::Serialize)]
        pub struct Bytes(#[serde(with = "serde_bytes_like")] pub Vec<u8>);
        mod serde_bytes_like {
            pub fn serialize<S: serde::Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
                s.serialize_bytes(v)
            }
        }
    }

    #[test]
    fn matches_a_value_tree() {
        same(&S {
            zz: vec![
                E::Unit,
                E::New(7),
                E::Tuple(1, "x".into()),
                E::Struct {
                    zeta: 1,
                    alpha: None,
                },
            ],
            b: bytes::Bytes(vec![0, 255]),
            q: (),
            m: [(10, "ten"), (9, "nine")].into_iter().collect(),
            n: -5,
            f: 1.5,
            nan: f64::NAN,
            c: 'é',
            aa: Some("é\n".into()),
        });
        same(&json!({"b": [1, {"y": null, "x": "\u{7f}"}], "a": 1.0e30}));
        same(&Vec::<u8>::new());
    }

    #[test]
    fn a_repeated_key_keeps_its_last_value() {
        struct Twice;
        impl Serialize for Twice {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                use serde::ser::SerializeMap;
                let mut m = s.serialize_map(None)?;
                m.serialize_entry("b", &1)?;
                m.serialize_entry("a", &2)?;
                m.serialize_entry("b", &3)?;
                m.end()
            }
        }
        same(&Twice);
    }
}
