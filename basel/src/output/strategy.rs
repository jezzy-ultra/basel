#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Write {
    Smart,
    Skip,
    Force,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Decision {
    Create,
    Recreate,
    Update,
    Overwrite,
    Skip,
    Conflict,
}

impl Decision {
    pub(crate) const fn should_write(self) -> bool {
        use Decision::{Conflict, Create, Overwrite, Recreate, Skip, Update};

        match self {
            Create | Recreate | Update | Overwrite => true,
            Skip | Conflict => false,
        }
    }

    pub(crate) const fn log_action(self) -> &'static str {
        use Decision::{Conflict, Create, Overwrite, Recreate, Skip, Update};

        match self {
            Create => "creating",
            Recreate => "recreating",
            Update => "updating",
            Overwrite => "overwriting",
            Skip => "skipped",
            Conflict => "conflict",
        }
    }
}

#[derive(Debug)]
pub(crate) enum FileStatus {
    NotTracked,
    Tracked {
        file_exists: bool,
        user_modified: bool,
        template_changed: bool,
        scheme_changed: bool,
    },
}

pub(crate) const fn decide(status: FileStatus, mode: Write) -> Decision {
    use Decision::{Conflict, Create, Overwrite, Recreate, Skip, Update};
    use FileStatus::{NotTracked, Tracked};

    match (status, mode) {
        (NotTracked, _) => Create,
        (
            Tracked {
                file_exists: false, ..
            },
            _,
        ) => Recreate,
        (
            Tracked {
                user_modified: true,
                ..
            },
            Write::Force,
        ) => Overwrite,
        (
            Tracked {
                user_modified: true,
                ..
            },
            Write::Smart,
        ) => Conflict,
        (
            Tracked {
                user_modified: false,
                template_changed: false,
                scheme_changed: false,
                ..
            },
            _,
        )
        | (Tracked { .. }, Write::Skip) => Skip,
        (
            Tracked {
                user_modified: false,
                ..
            },
            _,
        ) => Update,
    }
}
