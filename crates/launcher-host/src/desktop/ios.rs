//! UIKit browser handoff. Never launches a subprocess or blocks the UIKit thread.

use std::{sync::mpsc, time::Duration};

use block2::RcBlock;
use objc2::runtime::Bool;
use objc2_foundation::{MainThreadMarker, NSDictionary, NSString, NSURL};
use objc2_ui_kit::UIApplication;

/// Called by the shared browser worker; reports UIKit's actual completion to the sign-in UI.
pub(super) fn open_url(url: &str) -> bool {
    if MainThreadMarker::new().is_some() {
        tracing::error!("iOS browser handoff must run on the browser worker");
        return false;
    }
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    if !matches!(parsed.scheme(), "https" | "http") || parsed.host_str().is_none() {
        return false;
    }
    let url = parsed.to_string();
    let (sender, receiver) = mpsc::sync_channel(1);
    dispatch2::DispatchQueue::main().exec_async(move || {
        let mtm = MainThreadMarker::new().expect("UIKit main queue");
        let Some(url) = (unsafe { NSURL::URLWithString(&NSString::from_str(&url)) }) else {
            let _ = sender.send(false);
            return;
        };
        let completion = RcBlock::new(move |opened: Bool| {
            let _ = sender.send(opened.as_bool());
        });
        // UIKit retains the completion block. All UIKit objects are created/used on its queue.
        unsafe {
            UIApplication::sharedApplication(mtm).openURL_options_completionHandler(
                &url,
                &NSDictionary::new(),
                Some(&completion),
            );
        }
    });
    receiver
        .recv_timeout(Duration::from_secs(30))
        .unwrap_or(false)
}
