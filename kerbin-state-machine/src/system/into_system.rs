use crate::system::System;

pub trait IntoSystem<Input, Data> {
    type System: System;

    fn into_system(self) -> Self::System;
}
