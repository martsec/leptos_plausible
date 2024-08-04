//! Track pageviews and send custom events to plausible
//!
//!
use gloo_net::http::Request;
use leptos::{self, document, logging::debug_warn, spawn_local, window};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use wasm_bindgen::JsValue;

use super::experiments::{use_experiment_props, ExperimentCtx};

/// Main intro class handling Plausible events API.
///
/// ```
/// # use leptos_plausible::Plausible;
/// # use std::collections::HashMap;
/// # async fn no_run() {
/// let p = Plausible::new("your_domain");
///
/// p.pageview().send().await;
///
/// let mut event = p.event("MyCustomEvent");
///
/// event.props(HashMap::from([
///         ("experiment".into(), "experiment_name".into()),
///         ("variant".into(), "A".into()),
///     ]));
///     
/// event.send().await;
///
/// # }
/// ```
///
/// Or if you use a self-hosted instance or a proxy url:
///
/// ```
/// # use leptos_plausible::Plausible;
/// # async fn no_run() {
/// let p = Plausible::new_private("your_domain", "https://your_plausible_instance.com");
/// # }
/// ```
///
#[derive(Clone, Debug)]
pub struct Plausible {
    /// This domain name you used when you added your site to your Plausible account
    domain: String,
    /// Plausibe url for custom instances. By `new()` constructor sets it as `https://plausible.io`
    plausible_url: String,
}

impl Plausible {
    // Handy methods
    //
    #[must_use]
    pub fn link_click(&self, outbound_url: &str) -> EventBuilder {
        self.build_event(EventName::OutboundLinkClick)
            .props(HashMap::from([(String::from("url"), outbound_url.into())]))
    }

    #[must_use]
    pub fn pageview(&self) -> EventBuilder {
        self.build_event(EventName::Pageview)
    }

    #[must_use]
    pub fn event(&self, name: &str) -> EventBuilder {
        self.build_event(EventName::Custom(name.into()))
    }
}

impl Plausible {
    #[must_use]
    pub fn new(domain: &str) -> Self {
        Self {
            domain: domain.into(),
            plausible_url: "https://plausible.io".into(),
        }
    }

    #[must_use]
    pub fn new_private(domain: &str, instance_url: &str) -> Self {
        Self {
            domain: domain.into(),
            plausible_url: instance_url.into(),
        }
    }

    fn default_url() -> String {
        window()
            .location()
            .href()
            .expect("ERROR with plausible event: url")
    }

    fn build_event(&self, name: EventName) -> EventBuilder {
        let header = PlausibleHeader {
            user_agent: window()
                .navigator()
                .user_agent()
                .unwrap_or_else(|_| "ERROR".into()),
            // FIXME this is wrong
            x_forwarded_for: window()
                .location()
                .href()
                .expect("Error with plausible event"),
        };
        let referrer = document().referrer();
        let body = PlausiblePayload {
            name: name.into(),
            url: Self::default_url(),
            domain: self.domain.clone(),
            referrer: if referrer.is_empty() {
                None
            } else {
                Some(referrer)
            },
            props: None,
            revenue: None,
            screen_width: None,
        };

        EventBuilder {
            header,
            body,
            plausible_url: self.plausible_url.clone(),
        }
        .experiments()
    }
}

// From https://github.com/goddtriffin/plausible-rs/ under MIT license
#[derive(Debug, Clone)]
#[allow(clippy::module_name_repetitions)]
struct PlausibleHeader {
    pub user_agent: String,
    pub x_forwarded_for: String,
}

impl PlausibleHeader {
    #[must_use]
    pub const fn new(user_agent: String, x_forwarded_for: String) -> Self {
        Self {
            user_agent,
            x_forwarded_for,
        }
    }
}

/// Revenue data for this event.
/// This can be attached to goals and custom events to track revenue attribution.
///
/// In the case of an invalid currency or amount, the event is still recorded and
/// the API returns HTTP 202, but revenue data associated with it is discarded.
///
/// Not available to the community edition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevenueValue {
    currency: String,
    amount: String,
}

impl RevenueValue {
    pub fn new(currency: &str, amount: &str) -> Self {
        Self {
            currency: currency.into(),
            amount: amount.into(),
        }
    }
}

// From https://github.com/goddtriffin/plausible-rs/ under MIT license
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PlausiblePayload {
    /// Name of the event
    pub name: String,
    /// Domain name of the site in plausible
    pub domain: String,
    /// URL of the page where the event was triggered.
    /// By default it will be javascript's `window.location.href`
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub referrer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen_width: Option<usize>,
    /// Custom properties only accepts scalar values such as strings, numbers and booleans.
    /// Data structures such as objects, arrays etc. aren't accepted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub props: Option<HashMap<String, PropValue>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revenue: Option<RevenueValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::module_name_repetitions)]
pub enum EventName {
    Pageview,
    OutboundLinkClick,
    FileDownload,
    Purchase,
    NotFound,
    Custom(String),
}

#[allow(clippy::from_over_into)]
impl Into<String> for EventName {
    fn into(self) -> String {
        match self {
            Self::Pageview => "pageview".to_string(),
            Self::OutboundLinkClick => "Outbound Link: Click".to_string(),
            Self::FileDownload => "File Download".to_string(),
            Self::Purchase => "Purchase".to_string(),
            Self::NotFound => "404".to_string(),
            Self::Custom(s) => s,
        }
    }
}

#[derive(Debug, Clone)]
#[allow(clippy::module_name_repetitions)]
pub struct EventBuilder {
    plausible_url: String,
    header: PlausibleHeader,
    body: PlausiblePayload,
}

impl EventBuilder {
    /// Adds new properties overwriting if the key already exists
    pub fn props(mut self, props: HashMap<String, PropValue>) -> Self {
        match &mut self.body.props {
            None => self.body.props = Some(props),
            Some(existing) => {
                for (k, v) in props.clone().drain() {
                    existing.insert(k, v);
                }
            }
        };
        self
    }

    pub fn prop(self, name: &str, value: PropValue) -> Self {
        self.props(HashMap::from([(name.into(), value)]))
    }

    /// Adds revenue information. As of now it does not work
    /// on plausible community
    pub fn revenue(mut self, revenue: RevenueValue) -> Self {
        self.body.revenue = Some(revenue);
        self
    }

    pub fn referrer(mut self, referrer: String) -> Self {
        self.body.referrer = Some(referrer);
        self
    }

    pub const fn screen_width(mut self, screen_width: usize) -> Self {
        self.body.screen_width = Some(screen_width);
        self
    }

    /// Adds experiment properties. See [`ExperimentCtx`]
    ///
    /// WARNING: Does not run well inside `spawn_local` or `on:` functions
    // FIXME
    pub fn experiments(self) -> Self {
        match use_experiment_props() {
            Some(props) => self.props(props),
            None => self,
        }
    }
    pub fn set_experiment(self, experiment: Option<ExperimentCtx>) -> Self {
        match experiment {
            Some(e) => self.props(e.to_plausible()),
            None => self,
        }
    }

    /// Use it to specify custom locations for your page URL.
    ///
    /// For example if they include identifiers lile PII and UUID and you don't want to send those.
    /// You can send just `/user` to avoid sending sensitive data and improve
    /// Top Pages statistics.
    pub fn url(&mut self, url: &str) -> &mut Self {
        let url_with_params = format!(
            "{url}{}",
            window().location().search().expect("ERR with plausible")
        );
        self.body.url = url_with_params;
        self
    }

    pub async fn send(self) {
        // TODO disable sending event if localhost like done in the official script
        // TODO don't send pageview event if already visited before (window.history)
        // FIXME this from_serde should work but returns JSValue(Object(...)) which fails
        //let body = JsValue::from_serde(&self.body).expect("ERR serializing");
        let body = JsValue::from_str(&serde_json::to_string(&self.body).expect("ERR"));

        let resp = Request::post(&format!("{}/api/event", self.plausible_url))
            .referrer_policy(web_sys::ReferrerPolicy::StrictOriginWhenCrossOrigin)
            .mode(web_sys::RequestMode::NoCors)
            .header("Cache-Control", "no-cache")
            .header("Content-Type", "application/json")
            .body(body);

        let resp = resp.expect("Error building the body").send().await;
    }

    /// Creates a `spawn_local` thread and sends the event.
    ///
    /// Use this function instead of [`send`] for simplicity
    /// unless you want to do more things in the local thread.
    pub fn send_local(self) {
        debug_warn!("Preparing plausible event: `{:?}`", &self);
        spawn_local(async move {
            self.send().await;
        });
    }
}

/// Custom properties only accepts scalar values such as strings, numbers and booleans.
/// Data structures such as objects, arrays etc. aren't accepted.
// From https://github.com/goddtriffin/plausible-rs/ under MIT license
// Implementation on how to constrain types easily from: https://stackoverflow.com/a/52582432/11767294
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PropValue {
    // string
    String(String),

    // bool
    Bool(bool),

    // numbers
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    Usize(usize),

    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    Isize(isize),

    F32(f32),
    F64(f64),
}

impl From<&str> for PropValue {
    fn from(s: &str) -> Self {
        Self::String(s.into())
    }
}
impl From<String> for PropValue {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<bool> for PropValue {
    fn from(b: bool) -> Self {
        Self::Bool(b)
    }
}

impl From<u8> for PropValue {
    fn from(u: u8) -> Self {
        Self::U8(u)
    }
}

impl From<u16> for PropValue {
    fn from(u: u16) -> Self {
        Self::U16(u)
    }
}

impl From<u32> for PropValue {
    fn from(u: u32) -> Self {
        Self::U32(u)
    }
}

impl From<u64> for PropValue {
    fn from(u: u64) -> Self {
        Self::U64(u)
    }
}

impl From<u128> for PropValue {
    fn from(u: u128) -> Self {
        Self::U128(u)
    }
}

impl From<usize> for PropValue {
    fn from(u: usize) -> Self {
        Self::Usize(u)
    }
}

impl From<i8> for PropValue {
    fn from(i: i8) -> Self {
        Self::I8(i)
    }
}

impl From<i16> for PropValue {
    fn from(i: i16) -> Self {
        Self::I16(i)
    }
}

impl From<i32> for PropValue {
    fn from(i: i32) -> Self {
        Self::I32(i)
    }
}

impl From<i64> for PropValue {
    fn from(i: i64) -> Self {
        Self::I64(i)
    }
}

impl From<i128> for PropValue {
    fn from(i: i128) -> Self {
        Self::I128(i)
    }
}

impl From<isize> for PropValue {
    fn from(i: isize) -> Self {
        Self::Isize(i)
    }
}

impl From<f32> for PropValue {
    fn from(f: f32) -> Self {
        Self::F32(f)
    }
}

impl From<f64> for PropValue {
    fn from(f: f64) -> Self {
        Self::F64(f)
    }
}
