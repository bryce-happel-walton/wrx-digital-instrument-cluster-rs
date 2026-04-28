pub mod global_state;
pub mod settings;

pub mod dependencies {
    use std::fmt::Display;

    include!(concat!(env!("OUT_DIR"), "/dependencies.rs"));

    impl Display for DependencyInfo {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            if let Some(git_rev) = self.git_rev {
                write!(f, "{} ({})", self.version, git_rev)
            } else {
                write!(f, "{}", self.version)
            }
        }
    }
}
