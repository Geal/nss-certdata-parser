use std::result;

use syntax::Value;
use reader::RawObject;

#[derive(Debug, Clone)]
pub enum Object {
    Trust(Trust),
    Certificate(Certificate),
}

pub type Blob = Vec<u8>;
pub type Asn1 = Blob;

#[derive(Debug, Clone)]
pub struct Trust {
    pub label: String,
    pub issuer: Asn1,
    pub serial: Asn1,
    pub tls_server_trust: TrustLevel,
    pub email_trust: TrustLevel,
    pub code_signing_trust: TrustLevel,
    pub md5: Option<Blob>,
    pub sha1: Option<Blob>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TrustLevel {
    Distrust,
    MustVerify,
    TrustedDelegator,
}

impl TrustLevel {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "CKT_NSS_NOT_TRUSTED" => Some(TrustLevel::Distrust),
            "CKT_NSS_MUST_VERIFY_TRUST" => Some(TrustLevel::MustVerify),
            "CKT_NSS_TRUSTED_DELEGATOR" => Some(TrustLevel::TrustedDelegator),
            _ => None
        }
    }
}

#[derive(Debug, Clone)]
pub struct Certificate {
    pub value: Asn1,
    pub label: String,
    pub issuer: Asn1,
    pub serial: Asn1,
    pub subject: Asn1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
// TODO: will impl'ing Display make this print more usefully?
pub struct TypeError {
    pub got: String,
    pub expected: &'static str,
    pub key: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
// TODO: will impl'ing Display make this print more usefully?
pub struct ValueError {
    pub got: String,
    pub attr_type: &'static str,
    pub key: &'static str,
}

quick_error!{
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum StructureError {
        MissingKey(key: &'static str) {
            description("missing key")
            from()
        }
        TypeError(err: TypeError) {
            description("unexpected attribute type")
            from()
        }
        ValueError(err: ValueError) {
            description("unexpected attribute value")
            from()
        }
    }
}
use self::StructureError::MissingKey;

pub type Result<T> = result::Result<T, StructureError>;

fn take_bin(obj: &mut RawObject, key: &'static str) -> Result<Vec<u8>> {
    match obj.remove(key) {
        None => Err(MissingKey(key)),
        Some(Value::Binary(val)) => Ok(val),
        Some(val) => Err(TypeError {
            got: val.into_type(),
            expected: "MULTILINE_OCTAL",
            key: key
        }.into()),
    } 
}
fn take_str(obj: &mut RawObject, key: &'static str) -> Result<String> {
    match obj.remove(key) {
        None => Err(MissingKey(key)),
        Some(Value::String(val)) => Ok(val),
        Some(val) => Err(TypeError {
            got: val.into_type(),
            expected: "UTF8",
            key: key
        }.into()),
    } 
}
fn take_tok<R, F>(obj: &mut RawObject, key: &'static str, exp_ty: &'static str, xlate: F)
                  -> Result<R>
    where F: for<'a> FnOnce(&'a str) -> Option<R>
{
    let type_error = |got_ty| Err(TypeError {
        got: got_ty,
        expected: exp_ty,
        key: key,
    }.into());
    match obj.remove(key) {
        None => Err(MissingKey(key)),
        Some(Value::Token(got_ty, val)) => if got_ty == exp_ty {
            match xlate(&val) {
                Some(res) => Ok(res),
                None => Err(ValueError {
                    got: val,
                    attr_type: exp_ty,
                    key: key
                }.into())
            }
        } else {
            type_error(got_ty)
        },
        Some(val) => type_error(val.into_type()),
    }
}

fn optionalize<T>(r: Result<T>) -> Result<Option<T>> {
    match r {
        Ok(thing) => Ok(Some(thing)),
        Err(MissingKey(_)) => Ok(None),
        Err(err) => Err(err)
    }
}

impl Certificate {
    pub fn from_raw(mut obj: RawObject) -> Result<Certificate> {
        let obj = &mut obj;
        try!(take_tok(obj, "CKA_CERTIFICATE_TYPE", "CK_CERTIFICATE_TYPE", |cert_type| {
            if cert_type == "CKC_X_509" { Some(()) } else { None }
        }));
        Ok(Certificate {
            value: try!(take_bin(obj, "CKA_VALUE")),
            label: try!(take_str(obj, "CKA_LABEL")),
            issuer: try!(take_bin(obj, "CKA_ISSUER")),
            serial: try!(take_bin(obj, "CKA_SERIAL_NUMBER")),
            subject: try!(take_bin(obj, "CKA_SUBJECT")),
        })
    }
}

fn take_trust_level(obj: &mut RawObject, key: &'static str) -> Result<TrustLevel> {
    take_tok(obj, key, "CKT_TRUST", TrustLevel::from_str)
}

impl Trust {
    pub fn from_raw(mut obj: RawObject) -> Result<Trust> {
        let obj = &mut obj;
        Ok(Trust {
            label: try!(take_str(obj, "CKA_LABEL")),
            issuer: try!(take_bin(obj, "CKA_ISSUER")),
            serial: try!(take_bin(obj, "CKA_SERIAL_NUMBER")),
            tls_server_trust: try!(take_trust_level(obj, "CKA_TRUST_SERVER_AUTH")),
            email_trust: try!(take_trust_level(obj, "CKA_TRUST_EMAIL_PROTECTION")),
            code_signing_trust: try!(take_trust_level(obj, "CKA_TRUST_CODE_SIGNING")),
            md5: try!(optionalize(take_bin(obj, "CKA_CERT_MD5_HASH"))),
            sha1: try!(optionalize(take_bin(obj, "CKA_CERT_SHA1_HASH"))),
        }) 
    }
}

enum ObjClass {
    Certificate,
    Trust,
    Other,
}

fn take_class(obj: &mut RawObject) -> Result<ObjClass> {
    take_tok(obj, "CKA_CLASS", "CK_OBJECT_CLASS", |cls| Some(match cls {
        "CKO_CERTIFICATE" => ObjClass::Certificate,
        "CKO_NSS_TRUST" => ObjClass::Trust,
        _ => ObjClass::Other,
    }))
}

impl Object {
    pub fn from_raw(mut obj: RawObject) -> Result<Option<Object>> {
        match try!(take_class(&mut obj)) {
            ObjClass::Certificate =>
                Ok(Some(Object::Certificate(try!(Certificate::from_raw(obj))))),
            ObjClass::Trust =>
                Ok(Some(Object::Trust(try!(Trust::from_raw(obj))))),
            // Ignore CKO_NSS_BUILTIN_ROOT_LIST (and any other unexpected objects?)
            _ => Ok(None),
        }
    }
}