pub trait Dto {
    fn get_current_user_id(&self) -> u64;
    fn set_current_user_id(&mut self, value: u64);
}
