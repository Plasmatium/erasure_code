use std::{
    cell::RefCell,
    ops::{Add, Sub, Mul, Div},
    rc::Rc,
};

use num_bigint::{BigInt, Sign};
use num_rational::BigRational;

#[derive(Debug, Clone)]
pub struct MyBigRational(Rc<RefCell<BigRational>>);

impl MyBigRational {
    pub fn new(br: BigRational) -> Self {
        Self(Rc::new(RefCell::new(br)))
    }
    pub fn from_bytes(bytes: &[u8]) -> Self {
        // TODO: add length check and return Result instead
        let len = bytes.len();
        let cut_pos_layout: [u8; 8] = [
            bytes[len-8], bytes[len-7], bytes[len-6], bytes[len-5],
            bytes[len-4], bytes[len-3], bytes[len-2], bytes[len-1],
        ];
        let cut_pos = usize::from_le_bytes(cut_pos_layout);

        let numer_bytes = &bytes[..cut_pos];
        let numer_sign = u8_to_sign(bytes[len-10]);
        let numer = BigInt::from_bytes_le(numer_sign, numer_bytes);

        let denom_bytes = &bytes[cut_pos..];
        let denom_sign = u8_to_sign(bytes[len-9]);
        let denom = BigInt::from_bytes_le(denom_sign, denom_bytes);

        let big_rational = BigRational::new(numer, denom);
        big_rational.into()
    }

    pub fn to_bytes(self) -> Vec<u8> {
        let big_rational = self.0.take();
        let (numer_sign, numer_bytes) = big_rational.numer().to_bytes_le();
        let (denom_sign, denom_bytes) = big_rational.denom().to_bytes_le();

        let numer_sign = sign_to_u8(numer_sign);
        let denom_sign = sign_to_u8(denom_sign);

        let cut_pos = numer_bytes.len();
        let cut_pos_layout = cut_pos.to_le_bytes();

        let mut ret = Vec::with_capacity(numer_bytes.len() + denom_bytes.len() + 2 + std::mem::size_of::<usize>());
        ret.extend(numer_bytes);
        ret.extend(denom_bytes);
        ret.extend([numer_sign, denom_sign]);
        ret.extend(cut_pos_layout);

        ret
    }
}

fn u8_to_sign(s: u8) -> Sign {
    match s {
        0 => Sign::Minus,
        2 => Sign::Plus,
        _ => Sign::NoSign,
    }
}

fn sign_to_u8(s: Sign) -> u8 {
    match s {
        Sign::Minus => 0,
        Sign::NoSign => 1,
        Sign::Plus => 2,
    }
}

impl Add for MyBigRational {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        let a = &*self.0.borrow();
        let b = &*rhs.0.borrow();
        Self::new(a + b)
    }
}

impl Sub for MyBigRational {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        let a = &*self.0.borrow();
        let b = &*rhs.0.borrow();
        Self::new(a - b)
    }
}

impl Mul for MyBigRational {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        let a = &*self.0.borrow();
        let b = &*rhs.0.borrow();
        Self::new(a * b)
    }
}

impl Div for MyBigRational {
    type Output = Self;

    fn div(self, rhs: Self) -> Self::Output {
        let a = &*self.0.borrow();
        let b = &*rhs.0.borrow();
        Self::new(a / b)
    }
}

impl From<BigInt> for MyBigRational {
    fn from(value: BigInt) -> Self {
        let a = value.into();
        Self::new(a)
    }
}

impl From<BigRational> for MyBigRational {
    fn from(value: BigRational) -> Self {
        Self::new(value)
    }
}

#[cfg(test)]
mod tests {
    use num_bigint::{BigInt, Sign};
    use num_rational::BigRational;

    use super::MyBigRational;

    #[test]
    fn test_from_into() {
        let x: BigInt = 3.into();
        let g = MyBigRational::from(x);
        println!("{g:?}")
    }

    #[test]
    fn test_deref() {
        let a = BigInt::from_bytes_le(Sign::Plus, &vec![1u8, 3, 45, 3, 254]);
        let b = BigInt::from_bytes_le(Sign::Plus, &vec![32u8, 65, 32, 6, 91, 44, 113]);
        let br1 = BigRational::new(a.clone(), 234125.into());
        let br2 = BigRational::new(b.clone(), a.clone());
        let br1 = MyBigRational::new(br1);
        let br2 = MyBigRational::new(br2);

        let br3 = br1 + BigInt::from(3).into();
    }
}
