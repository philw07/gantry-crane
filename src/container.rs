pub struct Container {
    pub name: String,
}

impl Container {
    pub fn with_name(name: &str) -> Self {
        Container {
            name: name.to_owned(),
        }
    }
}
