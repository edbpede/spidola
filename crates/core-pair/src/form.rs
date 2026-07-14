// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The one static page + one POST shape; AGPL §13 source link on every page.
//!
//! Everything the pairing server can say to a phone is rendered here, and every response
//! body passes through one private `shell` function. That is not tidiness — it is the
//! compliance mechanism.
//! AGPL §13 requires that users interacting with the program over a network be prominently
//! offered the Corresponding Source, and routing every byte through one function makes
//! serving a page without the offer *structurally impossible* rather than a thing reviewers
//! must remember. This is what TECH_SPEC §12 means by "keeps AGPL section 13 compliance
//! trivially true"; `every_served_page_offers_the_source` is the proof.
//!
//! The offer is a colophon, not a banner. A legal notice repeated atop every page is clutter
//! that trains the eye to skip it, which defeats the requirement it is trying to satisfy — so
//! it sits where a colophon sits, at caption size in the PRD §8.2 palette, with "Source code"
//! as a real link. Quiet, present on every page, and impossible to remove by accident.
//!
//! These pages are the only user-facing HTML in Spidola. They are self-contained by rule: no
//! CDN, no external font, no script. The TV has no guaranteed internet, and shipping
//! third-party network behavior would contradict the privacy posture the project states in
//! PRD §10 ("no third-party SDKs with network behavior").

use core_model::{Secret, StreamLocator};

use crate::error::Rejection;

/// The Corresponding Source offer, read from the manifest at compile time.
///
/// `env!` rather than a literal so the AGPL §13 link cannot drift away from the repository
/// the crate actually declares (`repository.workspace = true`).
const SOURCE_URL: &str = env!("CARGO_PKG_REPOSITORY");

/// The `kind` discriminant for a playlist submission. Spelled as `core_model::SourceKind`
/// serializes it (kebab-case), so the shell's add-source flow reads the same word we do.
const KIND_M3U_URL: &str = "m3u-url";
/// The `kind` discriminant for an Xtream account submission.
const KIND_XTREAM: &str = "xtream";

/// Field caps for the form body. The connection's body cap (`crate::server`) already bounds
/// the total; these bound the *shape*, so a 4 KiB body cannot become 4 000 fields.
const MAX_FIELDS: usize = 8;
/// Longest accepted field name. Ours are all under ten bytes.
const MAX_NAME_LEN: usize = 32;
/// Longest accepted field value. Playlist URLs carry embedded auth and get long; 2 KiB is
/// well past anything real and well under the body cap.
const MAX_VALUE_LEN: usize = 2048;

/// A pairing submission, validated and ready for the TV's add-source flow (PRD §6.1).
///
/// Parsed, not validated: the URLs are [`StreamLocator`]s, so a `Submission` that exists is
/// one the shell can act on. The Xtream password is a [`Secret`] — it redacts its own
/// `Debug` and zeroizes on drop, which is what makes this type safe to log by accident.
#[derive(Debug, PartialEq, Eq)]
pub enum Submission {
    /// An M3U/M3U8 playlist to fetch by URL.
    M3uUrl {
        /// The playlist URL, as typed.
        url: StreamLocator,
    },
    /// An Xtream Codes account.
    Xtream {
        /// The Xtream server base URL.
        server: StreamLocator,
        /// The account username (not secret; the password is [`Self::Xtream::password`]).
        username: String,
        /// The account password. Never logged, never echoed into HTML, never persisted here
        /// — the shell hands it to the host-secrets callback (TECH_SPEC §12).
        password: Secret,
    },
}

/// Why a submission was refused, in the words the phone shows.
///
/// Deliberately not a `thiserror` enum: this never becomes the `source` of anything and never
/// leaves the crate. It becomes pixels. Modeling it as an enum rather than passing message
/// strings around keeps the copy in one table where it can be reviewed against PRD §8.6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InvalidSubmission {
    /// The `kind` discriminant was missing or not one this form serves.
    UnknownKind,
    /// The playlist URL was blank or did not parse.
    PlaylistLink,
    /// The Xtream server URL was blank or did not parse.
    ServerLink,
    /// The Xtream username was blank.
    Username,
    /// The Xtream password was blank.
    Password,
}

impl InvalidSubmission {
    /// The message shown above the form.
    ///
    /// PRD §8.6 voice: says what happened and what to do, and never names a mechanism. This
    /// copy is owned by this module and is never user input, so it is inserted into the page
    /// without escaping.
    fn message(self) -> &'static str {
        match self {
            Self::UnknownKind => "Pick what you're adding, then try again.",
            Self::PlaylistLink => "That doesn't look like a link. It should start with http://",
            Self::ServerLink => {
                "That server doesn't look like a link. It should start with http://"
            }
            Self::Username => "Add the username for your account.",
            Self::Password => "Add the password for your account.",
        }
    }
}

/// The decoded name/value pairs of one `application/x-www-form-urlencoded` body.
///
/// Deliberately **not** `Debug`: between [`parse_urlencoded`] and [`submission_from`] this
/// holds the submitted password as a plain `String` — it is the one place in the crate where
/// a credential exists outside a [`Secret`]. A derived `Debug` would make `{:?}`-ing it into
/// a log the easiest mistake in the file, and TECH_SPEC §4.8 forbids exactly that. Tests
/// assert with `matches!` rather than `assert_eq!` for the same reason.
///
/// [`Default`] is the empty set, which is how `GET /` renders the form: a first visit is
/// simply a submission with nothing filled in yet.
#[derive(Default)]
pub(crate) struct Fields {
    pairs: Vec<(String, String)>,
}

impl Fields {
    /// The value of a field, or `""` when absent.
    ///
    /// Absent and blank collapse on purpose. For the token that is the security-relevant
    /// half: a missing token and a wrong one take one code path and produce one response, so
    /// there is no shape for a guesser to tell them apart by. For the rest it just means one
    /// blank check instead of two.
    pub(crate) fn get(&self, name: &str) -> &str {
        self.pairs
            .iter()
            .find(|(key, _)| key == name)
            .map_or("", |(_, value)| value.as_str())
    }
}

/// Parses an `application/x-www-form-urlencoded` body.
///
/// By hand, because the alternative is a dependency in the one process a stranger can talk
/// to. Caps are checked against the *encoded* lengths, before decoding — decoding only ever
/// shrinks input, so nothing here can allocate past the cap.
///
/// # Errors
/// Returns [`Rejection::Malformed`] for a pair without `=`, or a body past any of the caps.
/// One variant for all of them: they mean the same thing to the sender ("this is not our
/// form"), and the distinction is not worth a second status code.
pub(crate) fn parse_urlencoded(body: &str) -> Result<Fields, Rejection> {
    let mut pairs = Vec::new();
    for pair in body.split('&').filter(|pair| !pair.is_empty()) {
        if pairs.len() == MAX_FIELDS {
            return Err(Rejection::Malformed);
        }
        let (name, value) = pair.split_once('=').ok_or(Rejection::Malformed)?;
        if name.len() > MAX_NAME_LEN || value.len() > MAX_VALUE_LEN {
            return Err(Rejection::Malformed);
        }
        pairs.push((percent_decode(name), percent_decode(value)));
    }
    Ok(Fields { pairs })
}

/// Builds a [`Submission`] from the posted fields, or says why it cannot.
///
/// Only the fields the chosen `kind` owns are read, so the panel the user did not fill in
/// cannot contribute to (or invalidate) the one they did.
///
/// # Errors
/// Returns the [`InvalidSubmission`] whose message the form is re-rendered with.
pub(crate) fn submission_from(fields: &Fields) -> Result<Submission, InvalidSubmission> {
    match fields.get(FIELD_KIND) {
        KIND_M3U_URL => {
            let url = locator(fields.get(FIELD_URL)).ok_or(InvalidSubmission::PlaylistLink)?;
            Ok(Submission::M3uUrl { url })
        }
        KIND_XTREAM => {
            let server = locator(fields.get(FIELD_SERVER)).ok_or(InvalidSubmission::ServerLink)?;
            let username = fields.get(FIELD_USERNAME).trim();
            if username.is_empty() {
                return Err(InvalidSubmission::Username);
            }
            let password = fields.get(FIELD_PASSWORD);
            if password.is_empty() {
                return Err(InvalidSubmission::Password);
            }
            Ok(Submission::Xtream {
                server,
                username: username.to_owned(),
                password: Secret::new(password),
            })
        }
        _ => Err(InvalidSubmission::UnknownKind),
    }
}

/// The posted field carrying this session's token.
pub(crate) const FIELD_TOKEN: &str = "token";
/// The posted field carrying the kind discriminant.
const FIELD_KIND: &str = "kind";
/// The posted field carrying the playlist URL.
const FIELD_URL: &str = "url";
/// The posted field carrying the Xtream server URL.
const FIELD_SERVER: &str = "server";
/// The posted field carrying the Xtream username.
const FIELD_USERNAME: &str = "username";
/// The posted field carrying the Xtream password. Read once, into a [`Secret`], and never
/// prefilled back into the form.
const FIELD_PASSWORD: &str = "password";

/// Parses a locator, treating a blank field and an unparseable one the same way — from the
/// couch they are the same mistake.
fn locator(raw: &str) -> Option<StreamLocator> {
    StreamLocator::parse(raw).ok()
}

/// Percent-decodes one form value: `+` becomes a space, `%XX` becomes its byte.
///
/// A malformed escape is kept as a literal `%` rather than rejected, which is what browsers
/// do and what a user pasting a URL containing a bare `%` expects. Invalid UTF-8 is replaced
/// rather than rejected: this is a text field a person typed, not a protocol.
fn percent_decode(input: &str) -> String {
    let mut out: Vec<u8> = Vec::with_capacity(input.len());
    let mut bytes = input.bytes();
    while let Some(byte) = bytes.next() {
        match byte {
            b'+' => out.push(b' '),
            b'%' => {
                // Probe two hex digits on a clone, so a malformed escape consumes nothing.
                let mut probe = bytes.clone();
                match (
                    probe.next().and_then(hex_value),
                    probe.next().and_then(hex_value),
                ) {
                    (Some(hi), Some(lo)) => {
                        out.push((hi << 4) | lo);
                        bytes = probe;
                    }
                    _ => out.push(b'%'),
                }
            }
            other => out.push(other),
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The value of one hex digit, or `None` if it is not one.
fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Escapes text for insertion into HTML — element content and quoted attribute values alike.
///
/// The five characters that can break out of either context. `&` is handled by the same
/// match arm as the rest, so it cannot double-encode what a later arm produced.
fn escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// The page shell every response body is rendered through — see the module docs.
///
/// `title` and `body` are trusted: both are assembled by this module from its own copy, with
/// every user-supplied value already passed through [`escape`] at the point of interpolation.
/// Nothing outside this module may call it.
fn shell(title: &str, body: &str) -> String {
    format!(
        "<!DOCTYPE html>\n\
         <html lang=\"en\">\n\
         <head>\n\
         <meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1, viewport-fit=cover\">\n\
         <meta name=\"color-scheme\" content=\"dark\">\n\
         <meta name=\"robots\" content=\"noindex, nofollow\">\n\
         <title>{title} · Spidola</title>\n\
         <style>{STYLE}</style>\n\
         </head>\n\
         <body>\n\
         <main class=\"card\">\n{body}</main>\n\
         <footer class=\"colophon\">Spidola · Free software (AGPL-3.0) · \
         <a href=\"{SOURCE_URL}\">Source code</a></footer>\n\
         </body>\n\
         </html>\n"
    )
}

/// The form (`GET /`), and the form again when a submission bounced (`POST /` → 400).
///
/// `prefill` carries the values back so a person does not retype a URL on a phone keyboard —
/// every one of them escaped on the way in. The password field is not among them: it has no
/// `value` attribute in any rendering of this page.
pub(crate) fn form_page(prefill: &Fields, invalid: Option<InvalidSubmission>) -> String {
    let alert = invalid.map_or_else(String::new, |reason| {
        format!(
            "<p class=\"alert\" role=\"alert\">{}</p>\n",
            reason.message()
        )
    });
    let xtream = prefill.get(FIELD_KIND) == KIND_XTREAM;
    let (m3u_checked, xtream_checked) = if xtream {
        ("", " checked")
    } else {
        (" checked", "")
    };
    let url = escape(prefill.get(FIELD_URL));
    let server = escape(prefill.get(FIELD_SERVER));
    let username = escape(prefill.get(FIELD_USERNAME));

    let body = format!(
        "<h1>Add a source</h1>\n\
         <p class=\"lede\">Type the code from your TV, then paste your details.</p>\n\
         {alert}\
         <form method=\"post\" action=\"/\" autocomplete=\"off\">\n\
         <div class=\"field\">\n\
         <label for=\"code\">Code from your TV</label>\n\
         <input id=\"code\" name=\"{FIELD_TOKEN}\" class=\"code\" type=\"text\" required \
         maxlength=\"6\" autocapitalize=\"characters\" autocorrect=\"off\" spellcheck=\"false\" \
         autocomplete=\"off\" placeholder=\"······\">\n\
         </div>\n\
         <input class=\"kind\" type=\"radio\" name=\"{FIELD_KIND}\" id=\"kind-playlist\" \
         value=\"{KIND_M3U_URL}\"{m3u_checked}>\n\
         <input class=\"kind\" type=\"radio\" name=\"{FIELD_KIND}\" id=\"kind-account\" \
         value=\"{KIND_XTREAM}\"{xtream_checked}>\n\
         <div class=\"segmented\">\n\
         <label for=\"kind-playlist\">Playlist link</label>\n\
         <label for=\"kind-account\">Xtream account</label>\n\
         </div>\n\
         <div class=\"panel panel-playlist\">\n\
         <div class=\"field\">\n\
         <label for=\"url\">Playlist link</label>\n\
         <input id=\"url\" name=\"{FIELD_URL}\" type=\"url\" inputmode=\"url\" \
         autocapitalize=\"none\" autocorrect=\"off\" spellcheck=\"false\" \
         placeholder=\"http://\" value=\"{url}\">\n\
         </div>\n\
         </div>\n\
         <div class=\"panel panel-account\">\n\
         <div class=\"field\">\n\
         <label for=\"server\">Server</label>\n\
         <input id=\"server\" name=\"{FIELD_SERVER}\" type=\"url\" inputmode=\"url\" \
         autocapitalize=\"none\" autocorrect=\"off\" spellcheck=\"false\" \
         placeholder=\"http://\" value=\"{server}\">\n\
         </div>\n\
         <div class=\"field\">\n\
         <label for=\"username\">Username</label>\n\
         <input id=\"username\" name=\"{FIELD_USERNAME}\" type=\"text\" autocapitalize=\"none\" \
         autocorrect=\"off\" spellcheck=\"false\" autocomplete=\"off\" value=\"{username}\">\n\
         </div>\n\
         <div class=\"field\">\n\
         <label for=\"password\">Password</label>\n\
         <input id=\"password\" name=\"{FIELD_PASSWORD}\" type=\"password\" autocomplete=\"off\">\n\
         </div>\n\
         </div>\n\
         <button class=\"submit\" type=\"submit\">Add source</button>\n\
         </form>\n"
    );
    shell("Add a source", &body)
}

/// The page after a submission lands (`POST /` → 200).
///
/// Names the kind and nothing else. Echoing the URL back would be a small reassurance and a
/// standing invitation to leak a credential-bearing string into a phone's back/forward cache;
/// the TV is about to show the real thing anyway (PRD §6.1).
pub(crate) fn confirmation_page(submission: &Submission) -> String {
    let what = match submission {
        Submission::M3uUrl { .. } => "playlist",
        Submission::Xtream { .. } => "account",
    };
    let body = format!(
        "<h1>That's on your TV now</h1>\n\
         <p class=\"lede\">Your {what} is waiting there — finish adding it on the TV.</p>\n\
         <p><a class=\"link\" href=\"/\">Add another</a></p>\n"
    );
    shell("That's on your TV now", &body)
}

/// A one-line page for everything that is not the form: a refused token, an unknown path, a
/// request over its budget.
///
/// `title` and `message` are this crate's own copy — never request data, which is what keeps
/// a hostile request line from being reflected back through [`shell`]'s trusted `body`.
pub(crate) fn notice_page(title: &str, message: &str) -> String {
    let body = format!(
        "<h1>{title}</h1>\n\
         <p class=\"lede\">{message}</p>\n\
         <p><a class=\"link\" href=\"/\">Add a source</a></p>\n"
    );
    shell(title, &body)
}

/// The whole stylesheet. Hand-written and inline: a phone that has reached this page is on a
/// LAN with a TV, and neither is promised an internet connection to fetch anything from.
///
/// PRD §8.2 palette, §8.3's system-face concession (a phone renders its own face best, and
/// this page is not the place to spend the display font's personality). The kind switch is a
/// checked-sibling selector rather than a script — there is no JavaScript on this page at
/// all, which is why the CSP can say `script-src 'none'` and mean it.
const STYLE: &str = "\
*,*::before,*::after{box-sizing:border-box}\
:root{--studio:#12151A;--set:#1C2129;--broadcast:#F1EFE9;--static:#8B94A3;--amber:#E3A44A;\
--fault:#C96F5B;--hairline:rgba(241,239,233,.10)}\
html{-webkit-text-size-adjust:100%}\
body{margin:0;min-height:100vh;padding:24px 20px calc(24px + env(safe-area-inset-bottom));\
display:flex;flex-direction:column;align-items:center;justify-content:center;gap:22px;\
background:var(--studio);color:var(--broadcast);font-size:16px;line-height:1.5;\
font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,system-ui,sans-serif}\
.card{width:100%;max-width:26rem;padding:28px 24px;background:var(--set);\
border:1px solid var(--hairline);border-radius:16px;box-shadow:0 18px 44px rgba(0,0,0,.45)}\
h1{margin:0 0 8px;font-size:1.5rem;font-weight:700;letter-spacing:-.01em}\
.lede{margin:0 0 22px;color:var(--static)}\
.alert{margin:0 0 22px;padding:12px 14px;border-radius:10px;font-size:.9375rem;\
background:rgba(201,111,91,.12);border:1px solid rgba(201,111,91,.42)}\
.field{margin-bottom:18px}\
label{display:block;margin-bottom:7px;font-size:.875rem;font-weight:600;color:var(--static)}\
input[type=text],input[type=url],input[type=password]{width:100%;min-height:48px;padding:12px 14px;\
font:inherit;font-size:16px;color:var(--broadcast);background:var(--studio);\
border:1px solid var(--hairline);border-radius:10px;appearance:none}\
input::placeholder{color:rgba(139,148,163,.55)}\
input:focus-visible{outline:2px solid var(--amber);outline-offset:2px;border-color:var(--amber)}\
.code{font-size:1.5rem;font-weight:700;letter-spacing:.4em;text-align:center;\
text-transform:uppercase;font-variant-numeric:tabular-nums;padding-left:.4em}\
.kind{position:absolute;width:1px;height:1px;opacity:0;pointer-events:none}\
.segmented{display:grid;grid-template-columns:1fr 1fr;gap:4px;margin-bottom:20px;padding:4px;\
background:var(--studio);border:1px solid var(--hairline);border-radius:12px}\
.segmented label{display:flex;align-items:center;justify-content:center;margin:0;min-height:44px;\
border-radius:8px;font-size:.9375rem;cursor:pointer;-webkit-user-select:none;user-select:none}\
#kind-playlist:checked~.segmented label[for=kind-playlist],\
#kind-account:checked~.segmented label[for=kind-account]{background:var(--set);\
color:var(--broadcast);box-shadow:inset 0 0 0 1px var(--hairline)}\
#kind-playlist:focus-visible~.segmented label[for=kind-playlist],\
#kind-account:focus-visible~.segmented label[for=kind-account]{outline:2px solid var(--amber);\
outline-offset:2px}\
.panel{display:none}\
#kind-playlist:checked~.panel-playlist,#kind-account:checked~.panel-account{display:block}\
.submit{width:100%;min-height:52px;margin-top:4px;font:inherit;font-size:1rem;font-weight:700;\
color:var(--studio);background:var(--amber);border:0;border-radius:10px;cursor:pointer;\
appearance:none}\
.submit:active{opacity:.85}\
.submit:focus-visible{outline:2px solid var(--amber);outline-offset:3px}\
.link{color:var(--broadcast);text-decoration:none;border-bottom:1px solid var(--hairline)}\
.link:focus-visible{outline:2px solid var(--amber);outline-offset:3px;border-radius:2px}\
.colophon{width:100%;max-width:26rem;text-align:center;font-size:.8125rem;line-height:1.7;\
color:var(--static);letter-spacing:.01em}\
.colophon a{color:var(--amber);text-decoration:none;padding-bottom:1px;\
border-bottom:1px solid rgba(227,164,74,.35)}\
.colophon a:focus-visible{outline:2px solid var(--amber);outline-offset:3px;border-radius:2px}\
";

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn fields(pairs: &[(&str, &str)]) -> Fields {
        Fields {
            pairs: pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
        }
    }

    fn playlist() -> Submission {
        Submission::M3uUrl {
            url: StreamLocator::parse("http://a.example/list.m3u").unwrap(),
        }
    }

    // --- The AGPL §13 compliance proof (module docs; TECH_SPEC §12, PRD §10) ---

    #[test]
    fn every_served_page_offers_the_source() {
        // Every page this module can render, enumerated. `shell` is private and every
        // renderer goes through it, so this list is the whole served surface: if a page is
        // added without the colophon, it can only be by removing it from `shell`, which
        // fails every case below at once.
        let pages = [
            form_page(&fields(&[]), None),
            form_page(&fields(&[]), Some(InvalidSubmission::PlaylistLink)),
            confirmation_page(&playlist()),
            notice_page("That code doesn't match", "Check the code on your TV."),
        ];
        for page in &pages {
            assert!(
                page.contains(SOURCE_URL),
                "a served page omits the Corresponding Source link:\n{page}"
            );
            assert!(
                page.contains("Source code</a>"),
                "the source link must be a real anchor, not bare text:\n{page}"
            );
            assert!(
                page.contains("AGPL-3.0"),
                "the colophon must name the license:\n{page}"
            );
        }
    }

    #[test]
    fn the_source_link_points_at_the_repository_the_crate_declares() {
        assert_eq!(SOURCE_URL, "https://github.com/edbpede/spidola");
    }

    // --- Escaping ---

    #[test]
    fn escape_neutralizes_every_breakout_character() {
        assert_eq!(escape(r#"<script>&"'"#), "&lt;script&gt;&amp;&quot;&#39;");
    }

    #[test]
    fn escape_does_not_double_encode() {
        // `&` is handled in the same match arm as everything else, so an ampersand this
        // function produced can never be re-escaped by a later arm.
        assert_eq!(escape("a & b"), "a &amp; b");
        assert_eq!(escape("&amp;"), "&amp;amp;");
    }

    #[test]
    fn a_prefilled_xss_payload_cannot_escape_its_attribute() {
        let payload = r#"" autofocus onfocus="alert(1)"><script>alert(document.domain)</script>"#;
        let page = form_page(
            &fields(&[("kind", KIND_M3U_URL), ("url", payload)]),
            Some(InvalidSubmission::PlaylistLink),
        );
        // The payload lands in `value="…"`, so the whole attack is whether its own `"` can
        // close that attribute early. It cannot, so the rest of the payload is inert text —
        // `onfocus=` does appear in the output, spelled with `&quot;`, and is just characters.
        assert!(
            !page.contains(r#"" autofocus"#),
            "the payload's quote closed the value attribute:\n{page}"
        );
        assert!(
            !page.contains("<script>"),
            "the payload opened a script element:\n{page}"
        );
        assert!(
            page.contains(
                "value=\"&quot; autofocus onfocus=&quot;alert(1)&quot;&gt;&lt;script&gt;\
                 alert(document.domain)&lt;/script&gt;\">"
            ),
            "the payload should survive fully escaped, so the user can see what they pasted \
             and fix it:\n{page}"
        );
    }

    #[test]
    fn prefill_returns_values_without_making_the_user_retype() {
        let page = form_page(
            &fields(&[
                ("kind", KIND_XTREAM),
                ("server", "http://panel.example:8080"),
                ("username", "alice"),
            ]),
            Some(InvalidSubmission::Password),
        );
        assert!(page.contains("value=\"http://panel.example:8080\""));
        assert!(page.contains("value=\"alice\""));
        assert!(page.contains("id=\"kind-account\" value=\"xtream\" checked"));
    }

    // --- The password is never echoed ---

    #[test]
    fn the_password_field_is_never_prefilled() {
        let page = form_page(
            &fields(&[
                ("kind", KIND_XTREAM),
                ("server", "http://panel.example"),
                ("username", "alice"),
                ("password", "hunter2-top-secret"),
            ]),
            Some(InvalidSubmission::ServerLink),
        );
        assert!(
            !page.contains("hunter2"),
            "the submitted password was echoed back into the form:\n{page}"
        );
        // The password input exists but carries no value attribute at all.
        assert!(page.contains("name=\"password\" type=\"password\" autocomplete=\"off\">"));
    }

    #[test]
    fn the_confirmation_page_echoes_no_credentials() {
        let submission = Submission::Xtream {
            server: StreamLocator::parse("http://panel.example:8080").unwrap(),
            username: "alice".to_owned(),
            password: Secret::new("hunter2-top-secret"),
        };
        let page = confirmation_page(&submission);
        for leaked in ["hunter2", "alice", "panel.example"] {
            assert!(
                !page.contains(leaked),
                "the confirmation page echoed {leaked:?}:\n{page}"
            );
        }
        assert!(page.contains("That's on your TV now"));
    }

    #[test]
    fn a_submissions_debug_output_redacts_the_password() {
        let submission = Submission::Xtream {
            server: StreamLocator::parse("http://panel.example").unwrap(),
            username: "alice".to_owned(),
            password: Secret::new("hunter2-top-secret"),
        };
        let rendered = format!("{submission:?}");
        assert!(
            !rendered.contains("hunter2"),
            "a submission logged with `{{:?}}` would leak the password: {rendered}"
        );
        assert!(rendered.contains("REDACTED"));
    }

    // --- Percent decoding ---

    #[test]
    fn decodes_plus_and_percent_escapes() {
        assert_eq!(percent_decode("a+b"), "a b");
        assert_eq!(percent_decode("%68%74%74%70"), "http");
        assert_eq!(
            percent_decode("http%3A%2F%2Fa.example%2Flist.m3u%3Fu%3D1%26p%3D2"),
            "http://a.example/list.m3u?u=1&p=2"
        );
        assert_eq!(percent_decode("caf%C3%A9"), "café"); // multi-byte UTF-8
    }

    #[test]
    fn a_malformed_escape_stays_literal() {
        // What a browser does, and what someone pasting a URL with a bare `%` expects.
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
        assert_eq!(percent_decode("%4"), "%4");
        assert_eq!(percent_decode("50%+off"), "50% off");
    }

    #[test]
    fn invalid_utf8_is_replaced_not_rejected() {
        assert_eq!(percent_decode("%FF"), "\u{FFFD}");
    }

    // --- Body parsing and its caps ---

    #[test]
    fn parses_a_real_form_body() {
        let parsed =
            parse_urlencoded("token=AB23CD&kind=m3u-url&url=http%3A%2F%2Fa.example%2Fl.m3u")
                .unwrap();
        assert_eq!(parsed.get("token"), "AB23CD");
        assert_eq!(parsed.get("kind"), "m3u-url");
        assert_eq!(parsed.get("url"), "http://a.example/l.m3u");
    }

    #[test]
    fn an_absent_field_reads_as_blank() {
        let parsed = parse_urlencoded("kind=xtream").unwrap();
        assert_eq!(parsed.get("token"), "");
        assert_eq!(parsed.get("nonexistent"), "");
    }

    #[test]
    fn an_empty_body_parses_to_nothing() {
        assert_eq!(parse_urlencoded("").unwrap().get("token"), "");
    }

    #[test]
    fn a_pair_without_an_equals_is_malformed() {
        assert!(matches!(
            parse_urlencoded("token"),
            Err(Rejection::Malformed)
        ));
        assert!(matches!(
            parse_urlencoded("a=1&bogus"),
            Err(Rejection::Malformed)
        ));
    }

    #[test]
    fn the_field_count_is_capped() {
        let body = (0..=MAX_FIELDS)
            .map(|i| format!("f{i}=v"))
            .collect::<Vec<_>>()
            .join("&");
        assert!(matches!(parse_urlencoded(&body), Err(Rejection::Malformed)));
    }

    #[test]
    fn field_name_and_value_lengths_are_capped() {
        let long_value = format!("url={}", "x".repeat(MAX_VALUE_LEN + 1));
        assert!(matches!(
            parse_urlencoded(&long_value),
            Err(Rejection::Malformed)
        ));
        let long_name = format!("{}=v", "n".repeat(MAX_NAME_LEN + 1));
        assert!(matches!(
            parse_urlencoded(&long_name),
            Err(Rejection::Malformed)
        ));
    }

    // --- Submission validation ---

    #[test]
    fn a_playlist_submission_parses_into_a_locator() {
        let parsed = submission_from(&fields(&[
            ("kind", KIND_M3U_URL),
            ("url", "http://a.example/list.m3u"),
        ]))
        .unwrap();
        assert_eq!(parsed, playlist());
    }

    #[test]
    fn an_xtream_submission_carries_its_password_as_a_secret() {
        let parsed = submission_from(&fields(&[
            ("kind", KIND_XTREAM),
            ("server", "http://panel.example:8080"),
            ("username", "  alice  "),
            ("password", "hunter2"),
        ]))
        .unwrap();
        let Submission::Xtream {
            server,
            username,
            password,
        } = parsed
        else {
            panic!("expected an Xtream submission");
        };
        assert_eq!(server.as_str(), "http://panel.example:8080");
        assert_eq!(username, "alice", "the username should be trimmed");
        assert_eq!(password, Secret::new("hunter2"));
    }

    #[test]
    fn a_password_is_taken_verbatim_including_its_whitespace() {
        // Trimming a username is a kindness; trimming a password would silently change a
        // credential the headend will reject, and the user would never know why.
        let parsed = submission_from(&fields(&[
            ("kind", KIND_XTREAM),
            ("server", "http://panel.example"),
            ("username", "alice"),
            ("password", " pass with spaces "),
        ]))
        .unwrap();
        let Submission::Xtream { password, .. } = parsed else {
            panic!("expected an Xtream submission");
        };
        assert_eq!(password, Secret::new(" pass with spaces "));
    }

    #[test]
    fn every_way_to_get_the_form_wrong_names_itself() {
        let cases: [(&[(&str, &str)], InvalidSubmission); 6] = [
            (&[], InvalidSubmission::UnknownKind),
            (&[("kind", "telepathy")], InvalidSubmission::UnknownKind),
            (&[("kind", KIND_M3U_URL)], InvalidSubmission::PlaylistLink),
            (
                &[("kind", KIND_M3U_URL), ("url", "not a url")],
                InvalidSubmission::PlaylistLink,
            ),
            (&[("kind", KIND_XTREAM)], InvalidSubmission::ServerLink),
            (
                &[
                    ("kind", KIND_XTREAM),
                    ("server", "http://panel.example"),
                    ("username", "  "),
                ],
                InvalidSubmission::Username,
            ),
        ];
        for (posted, expected) in cases {
            assert_eq!(
                submission_from(&fields(posted)),
                Err(expected),
                "posted: {posted:?}"
            );
        }
    }

    #[test]
    fn a_blank_password_is_refused_rather_than_stored_empty() {
        let refused = submission_from(&fields(&[
            ("kind", KIND_XTREAM),
            ("server", "http://panel.example"),
            ("username", "alice"),
            ("password", ""),
        ]));
        assert_eq!(refused, Err(InvalidSubmission::Password));
    }

    #[test]
    fn the_panel_the_user_did_not_fill_cannot_invalidate_the_one_they_did() {
        // Both panels post, always — the hidden one's stale junk must not matter.
        let parsed = submission_from(&fields(&[
            ("kind", KIND_M3U_URL),
            ("url", "http://a.example/list.m3u"),
            ("server", "garbage"),
            ("username", ""),
            ("password", ""),
        ]))
        .unwrap();
        assert_eq!(parsed, playlist());
    }

    // --- The page is self-contained ---

    #[test]
    fn no_page_reaches_the_network_or_runs_a_script() {
        let page = form_page(&fields(&[]), None);
        for forbidden in ["<script", "http://cdn", "https://cdn", "@import", "fonts.g"] {
            assert!(
                !page.contains(forbidden),
                "the page must be self-contained, found {forbidden:?}"
            );
        }
        // The one external reference allowed is the AGPL §13 offer, which is a link the user
        // chooses to follow — not a subresource the page fetches.
        assert_eq!(page.matches("href=\"http").count(), 1);
    }

    #[test]
    fn the_form_is_reachable_on_a_phone() {
        let page = form_page(&fields(&[]), None);
        assert!(page.contains("name=\"viewport\""), "needs a viewport meta");
        assert!(
            STYLE.contains("font-size:16px"),
            "inputs must be 16px or larger, or iOS zooms the page on focus"
        );
        assert!(page.contains("<label for=\"code\">"), "inputs need labels");
    }
}
