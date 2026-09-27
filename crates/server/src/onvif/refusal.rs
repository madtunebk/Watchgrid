//! Why a camera refused a login, in words: "wrong password" is only one of
//! the reasons, and the one people check first even when it isn't it.

use super::digest;

pub struct Refusal<'a> {
    pub status: u16,
    /// `WWW-Authenticate` values of the reply.
    pub challenges: &'a [String],
    /// The SOAP fault, if any.
    pub body: &'a str,
    pub has_password: bool,
    /// Watchgrid asked for the camera's clock and got no usable answer, so
    /// the WS-Security timestamp went out in our time, not the camera's.
    pub clock_unreadable: bool,
}

pub fn explain(r: &Refusal<'_>) -> String {
    if !r.has_password {
        return "the camera wants a login, and no ONVIF password is saved for this camera".into();
    }
    let schemes: Vec<String> = r.challenges.iter().map(|c| digest::scheme(c)).collect();
    if !schemes.is_empty() && schemes.iter().all(|s| s == "basic") {
        return "the camera only offers HTTP Basic login, which Watchgrid doesn't use (it would send the password readable on the network)".into();
    }
    if schemes.iter().any(|s| s == "digest") && !r.challenges.iter().any(|c| digest::parse(c).is_some()) {
        let offered = r.challenges.iter().find(|c| digest::scheme(c) == "digest").map(|c| c.split(',').find(|p| p.to_ascii_lowercase().contains("algorithm")).unwrap_or("").trim().to_string()).unwrap_or_default();
        return format!("the camera asks for a Digest login Watchgrid can't answer yet ({offered})");
    }
    let base = format!("the camera rejected the ONVIF username or password (HTTP {})", r.status);
    let fault = r.body.to_ascii_lowercase();
    if ["messageexpired", "expired", "timestamp", "created"].iter().any(|w| fault.contains(w)) {
        format!("{base}: it says the login's timestamp is out of range. Check the camera's clock (NTP)")
    } else if r.clock_unreadable {
        format!("{base}. Watchgrid couldn't read the camera's clock, and a camera clock that is far off refuses correct passwords too")
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refusal<'a>(challenges: &'a [String], body: &'a str) -> Refusal<'a> {
        Refusal { status: 401, challenges, body, has_password: true, clock_unreadable: false }
    }

    #[test]
    fn says_which_reason_it_is() {
        let none: Vec<String> = vec![];
        assert!(explain(&Refusal { has_password: false, ..refusal(&none, "") }).contains("no ONVIF password is saved"));
        assert_eq!(explain(&refusal(&none, "")), "the camera rejected the ONVIF username or password (HTTP 401)");

        let basic = vec![r#"Basic realm="cam""#.to_string()];
        assert!(explain(&refusal(&basic, "")).contains("only offers HTTP Basic"));

        let sess = vec![r#"Digest realm="cam", nonce="n", algorithm=MD5-sess"#.to_string()];
        assert!(explain(&refusal(&sess, "")).contains("can't answer yet (algorithm=MD5-sess)"), "{}", explain(&refusal(&sess, "")));

        let expired = "<Fault><Code><Value>wsse:MessageExpired</Value></Code></Fault>";
        assert!(explain(&refusal(&none, expired)).contains("timestamp is out of range"));

        assert!(explain(&Refusal { clock_unreadable: true, ..refusal(&none, "") }).contains("couldn't read the camera's clock"));
    }

    #[test]
    fn a_digest_camera_that_still_refuses_is_a_wrong_password() {
        let digest = vec![r#"Digest realm="cam", nonce="n", qop="auth""#.to_string()];
        assert_eq!(explain(&refusal(&digest, "")), "the camera rejected the ONVIF username or password (HTTP 401)");
    }
}
