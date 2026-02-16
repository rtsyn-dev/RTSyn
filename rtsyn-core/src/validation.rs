pub struct Validator;

impl Validator {
    pub fn normalize_cores(cores: &mut Vec<usize>) {
        if cores.is_empty() {
            cores.push(0);
        }
    }

    pub fn validate_unit(unit: &str, valid_units: &[&str]) -> bool {
        valid_units.contains(&unit)
    }
}