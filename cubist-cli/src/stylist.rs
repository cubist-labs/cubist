use console::{style, StyledObject};

pub fn event<T>(name: T) -> StyledObject<T> {
    style(name).cyan().dim()
}

pub fn warning<T>(msg: T) -> StyledObject<T> {
    style(msg).yellow().bold()
}

pub fn sender<T>(msg: T) -> StyledObject<T> {
    style(msg).magenta().dim()
}

pub fn receiver<T>(msg: T) -> StyledObject<T> {
    style(msg).yellow().dim()
}
