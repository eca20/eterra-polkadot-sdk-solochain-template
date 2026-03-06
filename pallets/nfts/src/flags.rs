use core::marker::PhantomData;
use core::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not};

pub trait BitStorage:
    Copy
    + Default
    + PartialEq
    + Eq
    + BitAnd<Output = Self>
    + BitOr<Output = Self>
    + Not<Output = Self>
    + BitOrAssign
    + BitAndAssign
{
    fn count_ones(self) -> u32;
}

impl BitStorage for u64 {
    fn count_ones(self) -> u32 {
        u64::count_ones(self)
    }
}

impl BitStorage for u8 {
    fn count_ones(self) -> u32 {
        u8::count_ones(self)
    }
}

pub trait BitFlag: Copy {
    type Bits: BitStorage;

    const ALL: Self::Bits;

    fn bits(self) -> Self::Bits;

    fn empty() -> BitFlags<Self>
    where
        Self: Sized,
    {
        BitFlags::empty()
    }

    fn all() -> BitFlags<Self>
    where
        Self: Sized,
    {
        BitFlags::all()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BitFlags<E: BitFlag> {
    bits: E::Bits,
    _phantom: PhantomData<E>,
}

impl<E: BitFlag> Default for BitFlags<E> {
    fn default() -> Self {
        Self::empty()
    }
}

impl<E: BitFlag> BitFlags<E> {
    pub fn empty() -> Self {
        Self {
            bits: Default::default(),
            _phantom: PhantomData,
        }
    }

    pub fn all() -> Self {
        Self {
            bits: E::ALL,
            _phantom: PhantomData,
        }
    }

    pub fn from_bits(bits: E::Bits) -> Result<Self, ()> {
        if (bits & !E::ALL) != Default::default() {
            return Err(());
        }
        Ok(Self {
            bits,
            _phantom: PhantomData,
        })
    }

    pub fn bits(&self) -> E::Bits {
        self.bits
    }

    pub fn contains(&self, flag: E) -> bool {
        (self.bits & flag.bits()) != Default::default()
    }

    pub fn insert(&mut self, flag: E) {
        self.bits |= flag.bits();
    }

    pub fn remove(&mut self, flag: E) {
        self.bits &= !flag.bits();
    }

    pub fn len(&self) -> usize {
        self.bits.count_ones() as usize
    }

    pub fn is_empty(&self) -> bool {
        self.bits == Default::default()
    }
}
