pub trait Apply: Sized {
    fn apply<F>(mut self, f: F) -> Self
    where
        F: FnOnce(&mut Self),
    {
        f(&mut self);
        self
    }
}

impl<T> Apply for T {}

pub fn index_first_null(data: impl AsRef<[u8]>) -> Option<usize> {
    data.as_ref().iter().enumerate().find_map(
        |(index, char)| {
            if *char == 0 {
                Some(index)
            } else {
                None
            }
        },
    )
}
