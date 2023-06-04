/*
 * Copyright 2018 The Starlark in Rust Authors.
 * Copyright (c) Facebook, Inc. and its affiliates.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     https://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

//! Helpers for numerical values.

use std::cmp::Ordering;

use dupe::Dupe;
use either::Either;

use crate::collections::StarlarkHashValue;
use crate::values::type_repr::StarlarkTypeRepr;
use crate::values::types::float::StarlarkFloat;
use crate::values::types::int_or_big::StarlarkIntRef;
use crate::values::UnpackValue;
use crate::values::Value;
use crate::values::ValueLike;

/// [`Num`] represents a numerical value that can be unpacked from a [`Value`].
///
/// It's an intermediate representation that facilitates conversions between
/// numerical types and helps in implementation of arithmetical operations
/// between them.
#[derive(Clone, Debug, Dupe, Copy)]
pub(crate) enum Num<'v> {
    Int(StarlarkIntRef<'v>),
    Float(f64),
}

impl<'v> StarlarkTypeRepr for Num<'v> {
    fn starlark_type_repr() -> String {
        Either::<StarlarkIntRef, StarlarkFloat>::starlark_type_repr()
    }
}

impl<'v> UnpackValue<'v> for Num<'v> {
    fn expected() -> String {
        "int or float".to_owned()
    }

    #[allow(clippy::manual_map)]
    fn unpack_value(value: Value<'v>) -> Option<Self> {
        if let Some(i) = StarlarkIntRef::unpack_value(value) {
            Some(Num::Int(i))
        } else if let Some(f) = value.downcast_ref::<StarlarkFloat>() {
            Some(Num::Float(f.0))
        } else {
            None
        }
    }
}

impl<'v> Num<'v> {
    /// Get underlying value as float
    pub(crate) fn as_float(&self) -> f64 {
        match self {
            Self::Int(i) => i.to_f64(),
            Self::Float(f) => *f,
        }
    }

    pub(crate) fn f64_to_i32_exact(f: f64) -> Option<i32> {
        let i = f as i32;
        if i as f64 == f { Some(i) } else { None }
    }

    /// Get underlying value as int (if it can be precisely expressed as int)
    pub(crate) fn as_int(&self) -> Option<i32> {
        match self {
            Self::Int(i) => i.to_i32(),
            Self::Float(f) => Self::f64_to_i32_exact(*f),
        }
    }

    /// Get hash of the underlying number
    pub(crate) fn get_hash_64(self) -> u64 {
        fn float_hash(f: f64) -> u64 {
            if f.is_nan() {
                // all possible NaNs should hash to the same value
                0
            } else if f.is_infinite() {
                u64::MAX
            } else if f == 0.0 {
                // Both 0.0 and -0.0 need the same hash, but are both equal to 0.0
                0.0f64.to_bits()
            } else {
                f.to_bits()
            }
        }

        match (self.as_int(), self) {
            // equal ints and floats should have the same hash
            (Some(i), _) => i as u64,
            (None, Self::Float(f)) => float_hash(f),
            (None, Self::Int(StarlarkIntRef::Small(i))) => {
                // shouldn't happen - as_int() should have resulted in an int
                i as u64
            }
            (None, Self::Int(StarlarkIntRef::Big(b))) => {
                // Not perfect, but OK: `1000000000000000000000003` and `1000000000000000000000005`
                // flush to the same float, and neither is exact float,
                // so we could use better hash for such numbers.
                float_hash(b.to_f64())
            }
        }
    }

    pub(crate) fn get_hash(self) -> StarlarkHashValue {
        StarlarkHashValue::hash_64(self.get_hash_64())
    }
}

impl<'v> From<i32> for Num<'v> {
    fn from(i: i32) -> Self {
        Self::Int(StarlarkIntRef::Small(i))
    }
}

impl<'v> From<f64> for Num<'v> {
    fn from(f: f64) -> Self {
        Self::Float(f)
    }
}

/// This is total eq per starlark spec, not Rust's partial eq.
impl<'v> PartialEq for Num<'v> {
    fn eq(&self, other: &Self) -> bool {
        if let (Num::Int(a), Num::Int(b)) = (self, other) {
            a == b
        } else {
            StarlarkFloat::compare_impl(self.as_float(), other.as_float()) == Ordering::Equal
        }
    }
}

impl<'v> Eq for Num<'v> {}

impl<'v> PartialOrd for Num<'v> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<'v> Ord for Num<'v> {
    fn cmp(&self, other: &Self) -> Ordering {
        if let (Num::Int(a), Num::Int(b)) = (self, other) {
            a.cmp(b)
        } else {
            StarlarkFloat::compare_impl(self.as_float(), other.as_float())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_value() {
        assert!(Num::unpack_value(Value::new_bool(true)).is_none());
        assert!(Num::unpack_value(Value::new_bool(false)).is_none());
        assert!(Num::unpack_value(Value::new_empty_string()).is_none());
        assert!(Num::unpack_value(Value::new_none()).is_none());

        assert_eq!(
            Num::unpack_value(Value::new_int(0)).unwrap().as_int(),
            Some(0)
        );
        assert_eq!(
            Num::unpack_value(Value::new_int(42)).unwrap().as_int(),
            Some(42)
        );
        assert_eq!(
            Num::unpack_value(Value::new_int(-42)).unwrap().as_int(),
            Some(-42)
        );
    }

    #[test]
    fn test_conversion_to_float() {
        assert_eq!(Num::Int(StarlarkIntRef::Small(0)).as_float(), 0.0);
        assert_eq!(
            Num::Int(StarlarkIntRef::Small(i32::MAX)).as_float(),
            i32::MAX as f64
        );
        assert_eq!(
            Num::Int(StarlarkIntRef::Small(i32::MIN)).as_float(),
            i32::MIN as f64
        );

        assert_eq!(Num::Float(0.0).as_float(), 0.0);
        assert!(Num::Float(f64::NAN).as_float().is_nan());
    }

    #[test]
    fn test_conversion_to_int() {
        assert_eq!(Num::Int(StarlarkIntRef::Small(0)).as_int(), Some(0));
        assert_eq!(Num::Int(StarlarkIntRef::Small(42)).as_int(), Some(42));
        assert_eq!(Num::Int(StarlarkIntRef::Small(-42)).as_int(), Some(-42));

        assert_eq!(Num::Float(0_f64).as_int(), Some(0));
        assert_eq!(Num::Float(42_f64).as_int(), Some(42));
        assert_eq!(Num::Float(-42_f64).as_int(), Some(-42));

        assert_eq!(Num::Float(i32::MIN as f64).as_int(), Some(i32::MIN));
        assert_eq!(Num::Float(i32::MAX as f64).as_int(), Some(i32::MAX));

        assert_eq!(Num::Float(42.75).as_int(), None);
        assert_eq!(Num::Float(-42.75).as_int(), None);
        assert_eq!(Num::Float(f64::NAN).as_int(), None);
        assert_eq!(Num::Float(f64::INFINITY).as_int(), None);
        assert_eq!(Num::Float(f64::NEG_INFINITY).as_int(), None);
    }

    #[test]
    fn test_hashing() {
        assert_eq!(
            Num::Int(StarlarkIntRef::Small(0)).get_hash_64(),
            Num::Float(0.0).get_hash_64()
        );
        assert_eq!(
            Num::Int(StarlarkIntRef::Small(42)).get_hash_64(),
            Num::Float(42.0).get_hash_64()
        );

        assert_eq!(
            Num::Float(f64::INFINITY + f64::NEG_INFINITY).get_hash_64(),
            Num::Float(f64::NAN).get_hash_64()
        );
        assert_eq!(
            Num::Float("0.25".parse().unwrap()).get_hash_64(),
            Num::Float("25e-2".parse().unwrap()).get_hash_64()
        );
    }

    #[test]
    fn test_eq() {
        assert_eq!(Num::Float(f64::NAN), Num::Float(f64::NAN));
        assert_eq!(Num::Float(f64::INFINITY), Num::Float(f64::INFINITY));
        assert_eq!(Num::Int(StarlarkIntRef::Small(10)), Num::Float(10.0));
    }
}
