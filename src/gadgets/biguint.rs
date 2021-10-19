use std::marker::PhantomData;

use num::Integer;

use crate::field::extension_field::Extendable;
use crate::field::field_types::RichField;
use crate::gadgets::arithmetic_u32::U32Target;
use crate::iop::generator::{GeneratedValues, SimpleGenerator};
use crate::iop::target::{BoolTarget, Target};
use crate::iop::witness::{PartitionWitness, Witness};
use crate::plonk::circuit_builder::CircuitBuilder;

#[derive(Clone, Debug)]
pub struct BigUintTarget {
    pub limbs: Vec<U32Target>,
}

impl BigUintTarget {
    pub fn num_limbs(&self) -> usize {
        self.limbs.len()
    }

    pub fn get_limb(&self, i: usize) -> U32Target {
        self.limbs[i]
    }
}

impl<F: RichField + Extendable<D>, const D: usize> CircuitBuilder<F, D> {
    fn connect_biguint(&mut self, lhs: BigUintTarget, rhs: BigUintTarget) {
        let min_limbs = lhs.num_limbs().min(rhs.num_limbs());
        for i in 0..min_limbs {
            self.connect_u32(lhs.get_limb(i), rhs.get_limb(i));
        }

        for i in min_limbs..lhs.num_limbs() {
            self.assert_zero_u32(lhs.get_limb(i));
        }
        for i in min_limbs..rhs.num_limbs() {
            self.assert_zero_u32(rhs.get_limb(i));
        }
    }

    fn pad_biguints(
        &mut self,
        a: BigUintTarget,
        b: BigUintTarget,
    ) -> (BigUintTarget, BigUintTarget) {
        if a.num_limbs() > b.num_limbs() {
            let mut padded_b_limbs = b.limbs.clone();
            padded_b_limbs.extend(self.add_virtual_u32_targets(a.num_limbs() - b.num_limbs()));
            let padded_b = BigUintTarget {
                limbs: padded_b_limbs,
            };
            (a, padded_b)
        } else {
            let mut padded_a_limbs = a.limbs.clone();
            padded_a_limbs.extend(self.add_virtual_u32_targets(b.num_limbs() - a.num_limbs()));
            let padded_a = BigUintTarget {
                limbs: padded_a_limbs,
            };
            (padded_a, b)
        }
    }

    fn cmp_biguint(&mut self, a: BigUintTarget, b: BigUintTarget) -> BoolTarget {
        let (padded_a, padded_b) = self.pad_biguints(a.clone(), b.clone());

        let a_vec = padded_a.limbs.iter().map(|&x| x.0).collect();
        let b_vec = padded_b.limbs.iter().map(|&x| x.0).collect();

        self.list_le(a_vec, b_vec, 32)
    }

    fn add_virtual_biguint_target(&mut self, num_limbs: usize) -> BigUintTarget {
        let limbs = (0..num_limbs)
            .map(|_| self.add_virtual_u32_target())
            .collect();

        BigUintTarget { limbs }
    }

    // Add two `BigUintTarget`s.
    pub fn add_biguint(&mut self, a: BigUintTarget, b: BigUintTarget) -> BigUintTarget {
        let num_limbs = a.limbs.len();
        debug_assert!(b.limbs.len() == num_limbs);

        let mut combined_limbs = vec![];
        let mut carry = self.zero_u32();
        for i in 0..num_limbs {
            let (new_limb, new_carry) =
                self.add_three_u32(carry.clone(), a.limbs[i].clone(), b.limbs[i].clone());
            carry = new_carry;
            combined_limbs.push(new_limb);
        }
        combined_limbs[num_limbs] = carry;

        BigUintTarget {
            limbs: combined_limbs,
        }
    }

    // Subtract two `BigUintTarget`s. We assume that the first is larger than the second.
    pub fn sub_biguint(&mut self, a: BigUintTarget, b: BigUintTarget) -> BigUintTarget {
        let num_limbs = a.limbs.len();
        debug_assert!(b.limbs.len() == num_limbs);

        let mut result_limbs = vec![];

        let mut borrow = self.zero_u32();
        for i in 0..num_limbs {
            let (result, new_borrow) = self.sub_u32(a.limbs[i], b.limbs[i], borrow);
            result_limbs[i] = result;
            borrow = new_borrow;
        }
        // Borrow should be zero here.

        BigUintTarget {
            limbs: result_limbs,
        }
    }

    pub fn mul_biguint(&mut self, a: BigUintTarget, b: BigUintTarget) -> BigUintTarget {
        let num_limbs = a.limbs.len();
        debug_assert!(b.limbs.len() == num_limbs);

        let mut to_add = vec![vec![]; 2 * num_limbs];
        for i in 0..num_limbs {
            for j in 0..num_limbs {
                let (product, carry) = self.mul_u32(a.limbs[i], b.limbs[j]);
                to_add[i + j].push(product);
                to_add[i + j + 1].push(carry);
            }
        }

        let mut combined_limbs = vec![];
        let mut carry = self.zero_u32();
        for i in 0..2 * num_limbs {
            to_add[i].push(carry);
            let (new_result, new_carry) = self.add_many_u32(to_add[i].clone());
            combined_limbs.push(new_result);
            carry = new_carry;
        }
        combined_limbs.push(carry);

        BigUintTarget {
            limbs: combined_limbs,
        }
    }

    pub fn div_rem_biguint(
        &mut self,
        a: BigUintTarget,
        b: BigUintTarget,
    ) -> (BigUintTarget, BigUintTarget) {
        let num_limbs = a.limbs.len();
        let div = self.add_virtual_biguint_target(num_limbs);
        let rem = self.add_virtual_biguint_target(num_limbs);

        self.add_simple_generator(BigUintDivRemGenerator::<F, D> {
            a: a.clone(),
            b: b.clone(),
            div: div.clone(),
            rem: rem.clone(),
            _phantom: PhantomData,
        });

        let div_b = self.mul_biguint(div.clone(), b.clone());
        let div_b_plus_rem = self.add_biguint(div_b, rem.clone());
        self.connect_biguint(a, div_b_plus_rem);

        let cmp_rem_b = self.cmp_biguint(rem.clone(), b);
        self.assert_one(cmp_rem_b.target);

        (div, rem)
    }
}

#[derive(Debug)]
struct BigUintDivRemGenerator<F: RichField + Extendable<D>, const D: usize> {
    a: BigUintTarget,
    b: BigUintTarget,
    div: BigUintTarget,
    rem: BigUintTarget,
    _phantom: PhantomData<F>,
}

impl<F: RichField + Extendable<D>, const D: usize> SimpleGenerator<F>
    for BigUintDivRemGenerator<F, D>
{
    fn dependencies(&self) -> Vec<Target> {
        self.a
            .limbs
            .iter()
            .map(|&l| l.0)
            .chain(self.b.limbs.iter().map(|&l| l.0))
            .collect()
    }

    fn run_once(&self, witness: &PartitionWitness<F>, out_buffer: &mut GeneratedValues<F>) {
        let a = witness.get_biguint_target(self.a.clone());
        let b = witness.get_biguint_target(self.b.clone());
        let (div, rem) = a.div_rem(&b);

        out_buffer.set_biguint_target(self.div.clone(), div);
        out_buffer.set_biguint_target(self.rem.clone(), rem);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_biguint_add() {}
}
