use serde::Serialize;


#[derive(Copy, Clone, Debug, Serialize)]
pub struct Daily {
    opening: Option<u16>,
    closing: Option<u16>,
    open: bool,
}

impl Daily {
    pub fn new_open(opening: u16, closing: u16) -> Self {
        Self {
            opening: Some(opening),
            closing: Some(closing),
            open: true,
        }
    }

    pub fn new_closed() -> Self {
        Self {
            opening: None,
            closing: None,
            open: true,
        }
    }

    pub fn open(&self) -> bool {
        self.open
    }

    pub fn opening(&self) -> Option<u16> {
        self.opening
    }

    pub fn closing(&self) -> Option<u16> {
        self.closing
    }
}
