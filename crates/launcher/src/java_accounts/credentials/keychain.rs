//! Generic-password records scoped to Java profile IDs, inaccessible in backups.
use core_foundation::{
    base::{CFType, CFTypeRef, TCFType},
    boolean::CFBoolean,
    data::CFData,
    dictionary::{CFDictionary, CFDictionaryRef},
    string::{CFString, CFStringRef},
};
use std::{io, ptr};

#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    static kSecClass: CFStringRef;
    static kSecClassGenericPassword: CFStringRef;
    static kSecAttrService: CFStringRef;
    static kSecAttrAccount: CFStringRef;
    static kSecAttrAccessible: CFStringRef;
    static kSecAttrAccessibleWhenUnlockedThisDeviceOnly: CFStringRef;
    static kSecValueData: CFStringRef;
    static kSecReturnData: CFStringRef;
    fn SecItemCopyMatching(query: CFDictionaryRef, result: *mut CFTypeRef) -> i32;
    fn SecItemAdd(query: CFDictionaryRef, result: *mut CFTypeRef) -> i32;
    fn SecItemUpdate(query: CFDictionaryRef, attributes: CFDictionaryRef) -> i32;
    fn SecItemDelete(query: CFDictionaryRef) -> i32;
}

fn key(value: CFStringRef) -> CFString {
    // Security exports immortal CFString objects; get-rule retains our reference.
    unsafe { CFString::wrap_under_get_rule(value) }
}
fn pairs(id: &str) -> Vec<(CFString, CFType)> {
    unsafe {
        vec![
            (key(kSecClass), key(kSecClassGenericPassword).as_CFType()),
            (
                key(kSecAttrService),
                CFString::new("app.cinnabar.java.accounts").as_CFType(),
            ),
            (key(kSecAttrAccount), CFString::new(id).as_CFType()),
        ]
    }
}
fn status(code: i32) -> io::Result<()> {
    if code == 0 {
        return Ok(());
    }
    let kind = if code == -25300 {
        io::ErrorKind::NotFound
    } else {
        io::ErrorKind::PermissionDenied
    };
    Err(io::Error::new(
        kind,
        format!("Java Keychain operation failed ({code}); unlock the device and retry"),
    ))
}
pub(super) fn read(id: &str) -> io::Result<Vec<u8>> {
    let mut query = pairs(id);
    query.push((
        key(unsafe { kSecReturnData }),
        CFBoolean::true_value().as_CFType(),
    ));
    let query = CFDictionary::from_CFType_pairs(&query);
    let mut result = ptr::null();
    status(unsafe { SecItemCopyMatching(query.as_concrete_TypeRef(), &mut result) })?;
    if result.is_null() {
        return Err(io::Error::other("Java Keychain returned no credentials"));
    }
    let value = unsafe { CFType::wrap_under_create_rule(result) };
    let data = value
        .downcast::<CFData>()
        .ok_or_else(|| io::Error::other("Invalid Java Keychain record"))?;
    Ok(data.bytes().to_vec())
}
pub(super) fn write(id: &str, bytes: &[u8]) -> io::Result<()> {
    let attrs = unsafe {
        vec![
            (key(kSecValueData), CFData::from_buffer(bytes).as_CFType()),
            (
                key(kSecAttrAccessible),
                key(kSecAttrAccessibleWhenUnlockedThisDeviceOnly).as_CFType(),
            ),
        ]
    };
    let mut all = pairs(id);
    all.extend(attrs.iter().cloned());
    let add = CFDictionary::from_CFType_pairs(&all);
    let code = unsafe { SecItemAdd(add.as_concrete_TypeRef(), ptr::null_mut()) };
    if code != -25299 {
        return status(code);
    }
    let query = CFDictionary::from_CFType_pairs(&pairs(id));
    let attrs = CFDictionary::from_CFType_pairs(&attrs);
    status(unsafe { SecItemUpdate(query.as_concrete_TypeRef(), attrs.as_concrete_TypeRef()) })
}
pub(super) fn delete(id: &str) -> io::Result<()> {
    let query = CFDictionary::from_CFType_pairs(&pairs(id));
    let code = unsafe { SecItemDelete(query.as_concrete_TypeRef()) };
    if code == -25300 { Ok(()) } else { status(code) }
}
