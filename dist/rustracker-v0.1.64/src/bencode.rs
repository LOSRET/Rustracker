use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Value {
    Bytes(Vec<u8>),
    Integer(i64),
    List(Vec<Value>),
    Dictionary(BTreeMap<Vec<u8>, Value>),
}

impl Value {
    pub fn bytes(bytes: impl Into<Vec<u8>>) -> Self {
        Self::Bytes(bytes.into())
    }

    pub fn string(value: impl AsRef<str>) -> Self {
        Self::Bytes(value.as_ref().as_bytes().to_vec())
    }

    pub fn integer(value: impl Into<i64>) -> Self {
        Self::Integer(value.into())
    }

    pub fn dictionary(entries: impl IntoIterator<Item = (impl Into<Vec<u8>>, Value)>) -> Self {
        let mut map = BTreeMap::new();
        for (key, value) in entries {
            map.insert(key.into(), value);
        }
        Self::Dictionary(map)
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.write_to(&mut out);
        out
    }

    fn write_to(&self, out: &mut Vec<u8>) {
        match self {
            Self::Bytes(bytes) => {
                out.extend_from_slice(bytes.len().to_string().as_bytes());
                out.push(b':');
                out.extend_from_slice(bytes);
            }
            Self::Integer(value) => {
                out.push(b'i');
                out.extend_from_slice(value.to_string().as_bytes());
                out.push(b'e');
            }
            Self::List(values) => {
                out.push(b'l');
                for value in values {
                    value.write_to(out);
                }
                out.push(b'e');
            }
            Self::Dictionary(values) => {
                out.push(b'd');
                for (key, value) in values {
                    out.extend_from_slice(key.len().to_string().as_bytes());
                    out.push(b':');
                    out.extend_from_slice(key);
                    value.write_to(out);
                }
                out.push(b'e');
            }
        }
    }
}

pub fn failure(message: impl AsRef<str>) -> Vec<u8> {
    Value::dictionary([(b"failure reason".to_vec(), Value::string(message.as_ref()))]).encode()
}

#[cfg(test)]
mod tests {
    use super::Value;

    #[test]
    fn encodes_dictionary_with_sorted_keys() {
        let encoded = Value::dictionary([
            (b"z".to_vec(), Value::integer(1)),
            (b"a".to_vec(), Value::string("ok")),
        ])
        .encode();

        assert_eq!(encoded, b"d1:a2:ok1:zi1ee");
    }
}
