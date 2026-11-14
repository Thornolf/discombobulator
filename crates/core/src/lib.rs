pub const APP_NAME: &str = "discombobulator";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_name_is_lowercase() {
        assert_eq!(APP_NAME, APP_NAME.to_lowercase());
    }
}
