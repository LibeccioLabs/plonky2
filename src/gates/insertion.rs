use crate::circuit_builder::CircuitBuilder;
use crate::field::extension_field::algebra::ExtensionAlgebra;
use crate::field::extension_field::target::ExtensionTarget;
use crate::field::extension_field::Extendable;
use crate::field::field::Field;
use crate::gates::gate::{Gate, GateRef};
use crate::generator::{SimpleGenerator, WitnessGenerator};
use crate::target::Target;
use crate::vars::{EvaluationTargets, EvaluationVars};
use crate::wire::Wire;
use crate::witness::PartialWitness;

/// A gate for inserting a value into a list at a non-deterministic location.
#[derive(Clone, Debug)]
pub(crate) struct InsertionGate<F: Extendable<D>, const D: usize> {
    pub vec_size: usize,
    pub _phantom: PhantomData<F>,
}

impl InsertionGate {
    pub fn new(vec_size: usize) -> GateRef<F, D> {
        let gate = Self {
            vec_size,
            _phantom: PhantomData,
        };
        GateRef::new(gate)
    }

    pub fn wires_insertion_index() -> usize {
        0
    }

    pub fn wires_element_to_insert() -> Range<usize> {
        1..D+1
    }

    pub fn wires_list_item(i: usize) -> Range<usize> {
        let start = (i + 1) * D + 1;
        start..start + D
    }

    fn start_of_output_wires(&self) -> usize {
        (self::vec_size + 1) * D + 1
    }

    pub fn wires_output_list_item(&self, i: usize) -> Range<usize> {
        let start = self::start_of_output_wires() + i * D;
        start..start + D
    }

    fn start_of_intermediate_wires(&self) -> usize {
        self::start_of_output_wires() + self::vec_size * D
    }

    fn intermediate_wires_per_round() -> {
        // D wires needed for each of insert_here, equality_dummy, new_item, new_item_plus_old_item,
        // already_inserted, and not_already_inserted
        6 * D
    }

    /// The wires corresponding to the "equality_dummy" variable in the gadget (non-gate) insert function.
    pub fn equality_dummy_for_round_r(r: usize) -> Range<usize> {
        let start = start_of_intermediate_wires() + r * wires_per_round() + D * intermediate_index;
        start..start + D
    }

    /// The wires corresponding to the "new_item" variable in the gadget (non-gate) insert function.
    pub fn new_item_for_round_r(r: usize) -> Range<usize> {
        let intermediate_index = 2;
        let start = start_of_intermediate_wires() + r * wires_per_round() + D * intermediate_index;
        start..start + D
    }

    /// The wires corresponding to the "new_item_plus_old_item" variable in the gadget (non-gate) insert function.
    pub fn new_item_plus_old_item_for_round_r(r: usize) -> Range<usize> {
        let intermediate_index = 3;
        let start = start_of_intermediate_wires() + r * wires_per_round() + D * intermediate_index;
        start..start + D
    }

    /// The wires corresponding to the "already_inserted" variable in the gadget (non-gate) insert function.
    pub fn already_inserted_for_round_r(r: usize) -> Range<usize> {
        let intermediate_index = 4;
        let start = start_of_intermediate_wires() + r * wires_per_round() + D * intermediate_index;
        start..start + D
    }

    /// The wires corresponding to the "not_already_inserted" variable in the gadget (non-gate) insert function.
    pub fn not_already_inserted_for_round_r(r: usize) -> Range<usize> {
        let intermediate_index = 5;
        let start = start_of_intermediate_wires() + r * wires_per_round() + D * intermediate_index;
        start..start + D
    }
}

impl<F: Extendable<D>, const D: usize> Gate<F, D> for InsertionGate {
    fn id(&self) -> String {
        format!("{:?}<D={}>", self, D)
    }

    fn eval_unfiltered(&self, vars: EvaluationVars<F, D>) -> Vec<F::Extension> {
        let insertion_index = vars.get_local_ext_algebra(Self::wires_insertion_index());
        let mut list_items = Vec::new();
        for i in 0..self::vec_size {
            list_items.push(vars.get_local_ext_algebra(Self::wires_list_item(i)));
        }

        let element_to_insert = vars.get_local_ext_algebra(Self::wires_element_to_insert());

        let mut constraints = Vec::new();

        let mut already_inserted = F::zero();
        for r in 0..self::vec_size {
            let cur_index = F::Extension::from_canonical_usize(r);
            
            let equality_dummy = vars.get_local_ext_algebra(Self::equality_dummy_for_round_r(r));

            let difference = cur_index - insertion_index;
            let insert_here = if difference == F::ZERO {
                F::ZERO
            } else {
                F::ONE
            };
            
            // The two equality constraints:
            constraints.extend(difference * equality_dummy - insert_here);
            constraints.extend((1 - insert_here) * difference);

            let mut new_item = insert_here * element_to_insert + already_inserted;
            if r > 0 {
                new_item += already_inserted * list_items[i - 1];
            }
            already_inserted += insert_here;

            new_item += (F::ONE - already_inserted) * list_items[i];
        }

        constraints
    }

    fn eval_unfiltered_recursively(
        &self,
        builder: &mut CircuitBuilder<F, D>,
        vars: EvaluationTargets<D>,
    ) -> Vec<ExtensionTarget<D>> {
        todo!()
    }

    fn generators(
        &self,
        gate_index: usize,
        local_constants: &[F],
    ) -> Vec<Box<dyn WitnessGenerator<F>>> {
        let gen = InsertionGenerator::<F, D> {
            gate_index,
            gate: self.clone(),
            _phantom: PhantomData,
        };
        vec![Box::new(gen)]
    }

    fn num_wires(&self) -> usize {
        let num_input_wires = self.vec_size + 1; // the original vector, and the insertion index
        let num_output_wires = self.vec_size + 1; // the final vector, with the inserted element
        let num_intermediate_wires = 6 * self.vec_size; // six intermediate variables needed for each element of the vector
        self.vec_size + 1
    }

    fn num_constants(&self) -> usize {
        0
    }

    fn degree(&self) -> usize {
        1
    }

    fn num_constraints(&self) -> usize {
        1
    }
}

#[derive(Debug)]
struct InsertionGenerator<F: Field> {
    gate_index: usize,
    gate: InterpolationGate<F, D>,
    _phantom: PhantomData<F>,
}

impl<F: Field> SimpleGenerator<F> for InsertionGenerator<F> {
    fn dependencies(&self) -> Vec<Target> {
        Vec::new()
    }

    fn run_once(&self, _witness: &PartialWitness<F>) -> PartialWitness<F> {
        let wire = Wire {
            gate: self.gate_index,
            input: ConstantGate::WIRE_OUTPUT,
        };
        PartialWitness::singleton_target(Target::Wire(wire), self.constant)
    }
}

#[cfg(test)]
mod tests {
    
}
