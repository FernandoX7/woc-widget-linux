use std::{path::PathBuf, time::Duration};

use cosmic::{
    app::Task,
    iced::{self, Length, Limits, Subscription},
    widget::{self, autosize},
    Element,
};
use wockit::{
    error::FetchError,
    models::{CryptoQuote, MenuBarDisplayMode, StatusResponse},
    services::{CryptoService, StatusService},
};

use crate::{
    activation,
    runtime::{reserved_width, RuntimeState},
    settings::{self, AppletSettings},
};

const APP_ID: &str = "io.github.fernandox7.wocplayercount.cosmic-applet";
const SYMBOLIC_ICON: &str = "io.github.fernandox7.wocplayercount-symbolic";

pub struct Applet {
    core: cosmic::Core,
    settings_path: PathBuf,
    settings: AppletSettings,
    runtime: RuntimeState,
    status_in_flight: bool,
    quote_in_flight: bool,
    token_sender:
        Option<calloop::channel::Sender<cosmic::applet::token::subscription::TokenRequest>>,
}

#[derive(Clone, Debug)]
pub enum Message {
    SettingsTick,
    SettingsLoaded(AppletSettings),
    StatusTick,
    StatusFinished(Result<StatusResponse, FetchError>),
    QuoteTick,
    QuoteFinished(Result<CryptoQuote, FetchError>),
    FreshnessTick,
    Activate,
    Token(cosmic::applet::token::subscription::TokenUpdate),
}

impl cosmic::Application for Applet {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(core: cosmic::Core, _flags: ()) -> (Self, Task<Message>) {
        let settings_path = settings::default_path().unwrap_or_else(|_| PathBuf::new());
        let mut runtime = RuntimeState::default();
        runtime.status_started();
        let app = Self {
            core,
            settings_path: settings_path.clone(),
            settings: AppletSettings::default(),
            runtime,
            status_in_flight: true,
            quote_in_flight: true,
            token_sender: None,
        };
        (
            app,
            Task::batch([
                Task::perform(settings::load(settings_path), |settings| {
                    Message::SettingsLoaded(settings).into()
                }),
                Task::perform(
                    async { StatusService::live().fetch_status().await },
                    |result| Message::StatusFinished(result).into(),
                ),
                Task::perform(
                    async { CryptoService::live().fetch_quote().await },
                    |result| Message::QuoteFinished(result).into(),
                ),
            ]),
        )
    }

    fn view(&self) -> Element<'_, Message> {
        if self.settings.menu_bar_display_mode == MenuBarDisplayMode::IconOnly {
            return self
                .core
                .applet
                .icon_button(SYMBOLIC_ICON)
                .on_press_down(Message::Activate)
                .into();
        }

        let width = reserved_width(self.settings.menu_bar_display_mode);
        let presentation = self.runtime.presentation(
            self.settings.menu_bar_display_mode,
            self.settings.quote_interval(),
            std::time::Instant::now(),
        );
        let content = widget::container(self.core.applet.text(presentation.label))
            .center_x(Length::Fixed(width))
            .center_y(Length::Shrink)
            .width(Length::Fixed(width));
        let mut limits = Limits::NONE
            .min_width(width)
            .max_width(width)
            .min_height(1.0);
        if let Some(bounds) = self.core.applet.suggested_bounds {
            if bounds.height > 0.0 {
                limits = limits.max_height(bounds.height);
            }
        }
        let content: Element<'_, Message> =
            autosize::autosize(content, widget::Id::new("woc-label-autosize"))
                .limits(limits)
                .into();
        widget::mouse_area(content)
            .on_press(Message::Activate)
            .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            cosmic::applet::token::subscription::activation_token_subscription(APP_ID)
                .map(Message::Token),
            iced::time::every(Duration::from_secs(1)).map(|_| Message::SettingsTick),
            iced::time::every(self.settings.status_interval()).map(|_| Message::StatusTick),
            iced::time::every(self.settings.quote_interval()).map(|_| Message::QuoteTick),
            iced::time::every(Duration::from_secs(1)).map(|_| Message::FreshnessTick),
        ])
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SettingsTick => {
                let path = self.settings_path.clone();
                return Task::perform(settings::load(path), |settings| {
                    Message::SettingsLoaded(settings).into()
                });
            }
            Message::SettingsLoaded(settings) => self.settings = settings,
            Message::StatusTick if !self.status_in_flight => {
                self.status_in_flight = true;
                self.runtime.status_started();
                return Task::perform(
                    async { StatusService::live().fetch_status().await },
                    |result| Message::StatusFinished(result).into(),
                );
            }
            Message::StatusFinished(result) => {
                self.status_in_flight = false;
                self.runtime.status_finished(result);
            }
            Message::QuoteTick if !self.quote_in_flight => {
                self.quote_in_flight = true;
                return Task::perform(
                    async { CryptoService::live().fetch_quote().await },
                    |result| Message::QuoteFinished(result).into(),
                );
            }
            Message::QuoteFinished(result) => {
                self.quote_in_flight = false;
                self.runtime.quote_finished(result);
            }
            Message::FreshnessTick | Message::StatusTick | Message::QuoteTick => {}
            Message::Activate => {
                let request = cosmic::applet::token::subscription::TokenRequest {
                    app_id: activation::MAIN_APP_ID.to_owned(),
                    exec: activation::MAIN_EXECUTABLE.to_owned(),
                };
                if self
                    .token_sender
                    .as_ref()
                    .is_none_or(|sender| sender.send(request).is_err())
                {
                    activation::launch(activation::MAIN_EXECUTABLE, None);
                }
            }
            Message::Token(cosmic::applet::token::subscription::TokenUpdate::Init(sender)) => {
                self.token_sender = Some(sender);
            }
            Message::Token(cosmic::applet::token::subscription::TokenUpdate::ActivationToken {
                token,
                exec,
            }) => activation::launch(&exec, token.as_deref()),
            Message::Token(cosmic::applet::token::subscription::TokenUpdate::Finished) => {
                self.token_sender = None;
            }
        }
        Task::none()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}
