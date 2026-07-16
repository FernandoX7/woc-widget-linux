mod activation;
mod app;
mod runtime;
mod settings;

fn main() -> cosmic::iced::Result {
    cosmic::applet::run::<app::Applet>(())
}
