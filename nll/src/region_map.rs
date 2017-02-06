#![allow(dead_code)]

use env::Point;
use nll_repr::repr;
use region::Region;

pub struct RegionMap {
    num_vars: usize,
    use_constraints: Vec<(RegionVariable, Point)>,
    outlives_constraints: Vec<(RegionVariable, RegionVariable)>,
    in_assertions: Vec<(RegionVariable, Point)>,
    out_assertions: Vec<(RegionVariable, Point)>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RegionVariable {
    index: usize
}

pub struct UseConstraint {
    var: RegionVariable,
    contains: Point,
}

pub struct InAssertion {
    var: RegionVariable,
    contains: Point,
}

pub struct OutAssertion {
    var: RegionVariable,
    contains: Point,
}

impl RegionMap {
    pub fn new() -> Self {
        RegionMap {
            num_vars: 0,
            use_constraints: vec![],
            outlives_constraints: vec![],
            in_assertions: vec![],
            out_assertions: vec![],
        }
    }

    pub fn new_var(&mut self) -> RegionVariable {
        self.num_vars += 1;
        RegionVariable { index: self.num_vars - 1 }
    }

    pub fn instantiate_ty<T>(&mut self, ty: &repr::Ty<T>) -> repr::Ty<RegionVariable> {
        repr::Ty {
            name: ty.name,
            args: ty.args.iter().map(|a| self.instantiate_arg(a)).collect()
        }
    }

    fn instantiate_arg<T>(&mut self, arg: &repr::TyArg<T>) -> repr::TyArg<RegionVariable> {
        match *arg {
            repr::TyArg::Region(_) => repr::TyArg::Region(self.new_var()),
            repr::TyArg::Ty(ref t) => repr::TyArg::Ty(self.instantiate_ty(t)),
        }
    }

    pub fn use_ty(&mut self, ty: &repr::Ty<RegionVariable>, point: Point) {
        for_each_region_variable(ty, &mut |var| self.use_constraints.push((var, point)));
    }

    pub fn assert_in(&mut self, ty: &repr::Ty<RegionVariable>, point: Point) {
        for_each_region_variable(ty, &mut |var| self.in_assertions.push((var, point)));
    }

    pub fn assert_out(&mut self, ty: &repr::Ty<RegionVariable>, point: Point) {
        for_each_region_variable(ty, &mut |var| self.out_assertions.push((var, point)));
    }

    /// Create the constraints such that `sub_ty <: super_ty`. Here we
    /// assume that both types are instantiations of a common 'erased
    /// type skeleton', and hence that the regions we will encounter
    /// as we iterate line up prefectly.
    ///
    /// We also assume all regions are contravariant for the time
    /// being.
    pub fn subtype(&mut self,
                   sub_ty: &repr::Ty<RegionVariable>,
                   super_ty: &repr::Ty<RegionVariable>) {
        let mut sub_regions = vec![];
        for_each_region_variable(sub_ty, &mut |var| sub_regions.push(var));

        let mut super_regions = vec![];
        for_each_region_variable(super_ty, &mut |var| super_regions.push(var));

        assert_eq!(sub_regions.len(), super_regions.len());

        for (&sub_region, &super_region) in sub_regions.iter().zip(&super_regions) {
            self.outlives_constraints.push((sub_region, super_region));
        }
    }

    pub fn solve<'m>(&'m self) -> RegionSolution<'m> {
        RegionSolution::new(self)
    }
}

pub fn for_each_region_variable<OP>(ty: &repr::Ty<RegionVariable>, op: &mut OP)
    where OP: FnMut(RegionVariable)
{
    for arg in &ty.args {
        for_each_region_variable_in_arg(arg, op);
    }
}

fn for_each_region_variable_in_arg<OP>(arg: &repr::TyArg<RegionVariable>, op: &mut OP)
    where OP: FnMut(RegionVariable)
{
    match *arg {
        repr::TyArg::Ty(ref t) => for_each_region_variable(t, op),
        repr::TyArg::Region(var) => op(var),
    }
}

pub struct RegionSolution<'m> {
    region_map: &'m RegionMap,
    values: Vec<Region>
}

impl<'m> RegionSolution<'m> {
    pub fn new(region_map: &'m RegionMap) -> Self {
        let mut solution = RegionSolution {
            region_map: region_map,
            values: (0..region_map.num_vars).map(|_| Region::new()).collect(),
        };
        solution.find();
        solution
    }

    fn find(&mut self) {
        for &(var, point) in &self.region_map.use_constraints {
            self.values[var.index].add_point(point);
        }

        let mut changed = true;
        while changed {
            changed = false;

            // For each `'a: 'b` constraint, we load value of `'b` and
            // ensure that `'a` grows to accommodate it.
            for &(a, b) in &self.region_map.outlives_constraints {
                if a != b {
                    // If I cared about perf, we could avoid this clone
                    let b_value = self.values[b.index].clone();
                    if self.values[a.index].add_region(&b_value) {
                        changed = true;
                    }
                }
            }
        }
    }

    pub fn region(&self, var: RegionVariable) -> &Region {
        &self.values[var.index]
    }

    pub fn check(&self) -> usize {
        let mut errors = 0;

        for &(var, point) in &self.region_map.in_assertions {
            if !self.region(var).contains(point) {
                println!("error: region for `{:?}` does not contain `{:?}`",
                         var, point);
                println!("    region computed for `{:?}` is {:?}`",
                         var, self.region(var));
                errors += 1;
            }
        }

        for &(var, point) in &self.region_map.out_assertions {
            if self.region(var).contains(point) {
                println!("error: region for `{:?}` does not contain `{:?}`",
                         var, point);
                println!("    region computed for `{:?}` is {:?}`",
                         var, self.region(var));
                errors += 1;
            }
        }

        errors
    }
}
