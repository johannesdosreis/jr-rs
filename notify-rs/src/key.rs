use std::any::{Any, TypeId};
use std::fmt::{self, Debug};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

/// Identifies a widget among its siblings, so reconciliation can match it with
/// the element that already holds it. Two keys are equal when both the value
/// type and the value are equal.
#[derive(Clone)]
pub struct Key(Rc<dyn KeyValue>);

impl Key {
    pub fn new<V: Hash + Eq + Debug + 'static>(value: V) -> Key {
        Key(Rc::new(value))
    }
}

trait KeyValue: Debug {
    fn as_any(&self) -> &dyn Any;
    fn eq_value(&self, other: &dyn KeyValue) -> bool;
    fn hash_value(&self, state: &mut dyn Hasher);
}

impl<V: Hash + Eq + Debug + 'static> KeyValue for V {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn eq_value(&self, other: &dyn KeyValue) -> bool {
        other.as_any().downcast_ref::<V>() == Some(self)
    }

    fn hash_value(&self, mut state: &mut dyn Hasher) {
        TypeId::of::<V>().hash(&mut state);
        self.hash(&mut state);
    }
}

impl PartialEq for Key {
    fn eq(&self, other: &Key) -> bool {
        self.0.eq_value(&*other.0)
    }
}

impl Eq for Key {}

impl Hash for Key {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash_value(state);
    }
}

impl Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<&str> for Key {
    fn from(value: &str) -> Key {
        Key::new(value.to_owned())
    }
}

macro_rules! key_from {
    ($($ty:ty),*) => {
        $(impl From<$ty> for Key {
            fn from(value: $ty) -> Key {
                Key::new(value)
            }
        })*
    };
}

key_from!(String, i32, i64, u32, u64, usize);

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn keys_with_same_type_and_value_are_equal() {
        assert_eq!(Key::new(1u32), Key::new(1u32));
    }

    #[test]
    fn keys_with_different_value_types_differ() {
        assert_ne!(Key::new(1u32), Key::new(1i64));
    }

    #[test]
    fn str_key_equals_string_key() {
        assert_eq!(Key::from("a"), Key::new(String::from("a")));
    }

    #[test]
    fn equal_keys_hash_equally() {
        let set: HashSet<Key> = [Key::from(7i32), Key::new(7i32)].into();
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn debug_shows_the_value() {
        assert_eq!(format!("{:?}", Key::from("a")), "\"a\"");
        assert_eq!(format!("{:?}", Key::from(3usize)), "3");
    }
}
