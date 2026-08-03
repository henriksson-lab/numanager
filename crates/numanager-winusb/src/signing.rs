//! Self-signing for the generated WinUSB driver package, so the install is
//! silent on default-policy Windows instead of prompting "unverified publisher".
//!
//! This is a Rust port of libwdi's `pki.c` (vendored under
//! `third_party/libwdi/pki.c`), trimmed to exactly what a one-file WinUSB INF
//! package needs:
//!
//! 1. [`create_cat`] — build a security catalog (`.cat`) whose single member is
//!    the generated INF, hashed with the Authenticode SHA-1 file hash and tagged
//!    with the package `HWID1`/`OS`/`File`/`OSAttr` attributes (`CryptCAT*`).
//! 2. [`self_sign_file`] — create a self-signed code-signing certificate, trust
//!    it (LocalMachine `Root` + `TrustedPublisher`), sign the `.cat` with it
//!    (`SignerSignEx`, SHA-256), then **destroy the private key** so it can never
//!    be reused to sign anything else.
//! 3. [`remove_signing_cert`] — revoke that trust by deleting the cert from both
//!    stores (exposed at the crate root for the same reason libwdi ships a
//!    remover: we added a trust anchor, the user must be able to take it back).
//!
//! ## Security note
//!
//! Signing installs a self-signed certificate into the machine `Root` store —
//! a trust anchor. The private key is deleted immediately after signing (see
//! [`self_sign_file`]), matching libwdi, so it cannot be abused; but the public
//! cert remains trusted until [`remove_signing_cert`] removes it. All of this
//! requires Administrator rights (the caller is already elevation-gated).
//!
//! ## Status
//!
//! Ported faithfully from `pki.c` and compiles, but — like the install path it
//! serves — has **not been exercised on hardware** (needs a driverless device
//! and admin). Deliberate simplifications vs. `pki.c`, none affecting signature
//! validity: only the code-signing EKU extension is set (the cosmetic Alt-Name
//! and CPS policy extensions are dropped); the Authenticode opus/statement
//! attributes are omitted (`SIGNER_NO_ATTR`); RSA-2048 key.

#![allow(non_snake_case)]

use std::os::windows::io::AsRawHandle;

use numanager_core::{Error, ErrorCode, Result};
use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{GetLastError, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Security::Cryptography::Catalog::{
    CryptCATAdminCalcHashFromFileHandle, CryptCATClose, CryptCATOpen, CryptCATPersistStore,
    CryptCATPutAttrInfo, CryptCATPutCatAttrInfo, CryptCATPutMemberInfo,
};
use windows_sys::Win32::Security::Cryptography::Sip::SIP_INDIRECT_DATA;
use windows_sys::Win32::Security::Cryptography::{
    CertAddCertificateContextToStore, CertAddEncodedCertificateToStore, CertCloseStore,
    CertCreateSelfSignCertificate, CertDeleteCertificateFromStore, CertFindCertificateInStore,
    CertFreeCertificateContext, CertOpenStore, CertSetCertificateContextProperty, CertStrToNameW,
    CryptAcquireContextW, CryptDestroyKey, CryptEncodeObject, CryptGenKey, CryptReleaseContext,
    SignerFreeSignerContext, SignerSignEx, CERT_CONTEXT, CERT_EXTENSION, CERT_EXTENSIONS,
    CRYPT_ALGORITHM_IDENTIFIER, CRYPT_ATTRIBUTE_TYPE_VALUE, CRYPT_INTEGER_BLOB,
    CRYPT_KEY_PROV_INFO, HCERTSTORE, SIGNER_CERT, SIGNER_CERT_0, SIGNER_CERT_STORE_INFO,
    SIGNER_CONTEXT, SIGNER_FILE_INFO, SIGNER_SIGNATURE_INFO, SIGNER_SIGNATURE_INFO_0,
    SIGNER_SUBJECT_INFO, SIGNER_SUBJECT_INFO_0,
};

/// Subject (and container) identity for our self-signed certificate.
pub(crate) const CERT_SUBJECT: &str = "CN=numanager WinUSB (self-signed)";
const KEY_CONTAINER: &str = "numanager winusb key container";
const SHA1_LEN: usize = 20;

// --- Win32 ABI constants (windows-sys exposes these as plain integer aliases,
// but several are not re-exported by name, so we spell out the stable values). ---
const X509_ASN_ENCODING: u32 = 0x0000_0001;
const CERT_X500_NAME_STR: u32 = 3;
const CERT_STORE_PROV_SYSTEM_W: *const u8 = 10 as *const u8; // (LPCSTR)10
const CERT_SYSTEM_STORE_LOCAL_MACHINE: u32 = 0x0002_0000;
const CERT_STORE_ADD_REPLACE_EXISTING: u32 = 3;
const CERT_FRIENDLY_NAME_PROP_ID: u32 = 11;
const CERT_FIND_SUBJECT_NAME: u32 = 0x0002_0007;
const PROV_RSA_FULL: u32 = 1;
const AT_SIGNATURE: u32 = 2;
const CRYPT_MACHINE_KEYSET: u32 = 0x0000_0020;
const CRYPT_SILENT: u32 = 0x0000_0040;
const CRYPT_NEWKEYSET: u32 = 0x0000_0008;
const CRYPT_DELETEKEYSET: u32 = 0x0000_0010;
const CRYPT_EXPORTABLE: u32 = 0x0000_0001;
const CRYPT_VERIFYCONTEXT: u32 = 0xF000_0000;
const NTE_BAD_KEYSET: u32 = 0x8009_0016;
/// `X509_ENHANCED_KEY_USAGE` struct id for `CryptEncodeObject`, passed as a
/// `MAKEINTRESOURCE`-style `PCSTR`.
const X509_ENHANCED_KEY_USAGE: *const u8 = 36 as *const u8;
const CALG_SHA_256: u32 = 0x0000_800c;
const SIGNER_SUBJECT_FILE: u32 = 1;
const SIGNER_CERT_STORE: u32 = 2;
const SIGNER_CERT_POLICY_CHAIN: u32 = 2;
const SIGNER_NO_ATTR: u32 = 0;
const CRYPTCAT_OPEN_CREATENEW: u32 = 0x0000_0001;
const CRYPTCAT_ATTR_AUTHENTICATED: u32 = 0x1000_0000;
const CRYPTCAT_ATTR_NAMEASCII: u32 = 0x0000_0001;
const CRYPTCAT_ATTR_DATAASCII: u32 = 0x0001_0000;
const SPC_FILE_LINK_CHOICE: u32 = 3;

// OID string literals (ASCII, NUL-terminated) passed as PCSTR/PSTR.
const OID_CODE_SIGNING: &[u8] = b"1.3.6.1.5.5.7.3.3\0";
const OID_ENHANCED_KEY_USAGE: &[u8] = b"2.5.29.37\0";
const OID_RSA_SHA256RSA: &[u8] = b"1.2.840.113549.1.1.11\0";
const OID_OIWSEC_SHA1: &[u8] = b"1.3.14.3.2.26\0";
const OID_SPC_CAB_DATA: &[u8] = b"1.3.6.1.4.1.311.2.1.25\0";

/// From the inf2cat `/os` parameter — the OS list stamped into the catalog.
const CAT_OS: &str = "7_X86,7_X64,8_X86,8_X64,8_ARM,10_X86,10_X64,10_ARM";
/// Per-member OS attribute libwdi writes for every catalog entry.
const MEMBER_OSATTR: &str = "2:5.1,2:5.2,2:6.0,2:6.1";

/// The INF subject-type GUID (`DE351A42-8E59-11D0-8C47-00C04FC295EE`).
const INF_SUBJECT_GUID: GUID = GUID {
    data1: 0xDE35_1A42,
    data2: 0x8E59,
    data3: 0x11D0,
    data4: [0x8C, 0x47, 0x00, 0xC0, 0x4F, 0xC2, 0x95, 0xEE],
};

/// libwdi's `SPC_LINK` for the CAB/INF Authenticode link. The real type has a
/// 32-byte union; we only ever use the `pwszUrl`/`pwszFile` pointer arm (choice
/// `SPC_FILE_LINK_CHOICE`, value `L"<<<Obsolete>>>"`), so the trailing bytes pad
/// the struct out to the true ABI size without being read by the encoder.
#[repr(C)]
struct SpcLink {
    dwLinkChoice: u32,
    _pad: u32,
    pwsz: *const u16,
    _union_tail: [u8; 24],
}

/// libwdi's `CERT_ENHKEY_USAGE` (a.k.a. `CTL_USAGE`) — not in windows-sys.
#[repr(C)]
struct CertEnhKeyUsage {
    cUsageIdentifier: u32,
    rgpszUsageIdentifier: *mut *const u8,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn last_err(ctx: &str) -> Error {
    let code = unsafe { GetLastError() };
    Error::new(
        ErrorCode::Driver,
        format!("{ctx} failed (GetLastError=0x{code:08x})"),
    )
}

// ------------------------------------------------------------------- catalog

/// Create a `.cat` at `cat_path` whose single member is `inf_name` (found in
/// `search_dir`), tagged for driver-package verification against `hardware_id`.
/// Mirrors libwdi `CreateCat` for a one-file INF package.
pub(crate) fn create_cat(
    cat_path: &std::path::Path,
    hardware_id: &str,
    search_dir: &std::path::Path,
    inf_name: &str,
) -> Result<()> {
    let inf_path = search_dir.join(inf_name);
    let hash = authenticode_sha1(&inf_path)?;

    let cat_w = wide(&cat_path.to_string_lossy());
    let hwid_w = wide(&hardware_id.to_ascii_lowercase());
    let os_w = wide(CAT_OS);

    // SAFETY: FFI. The catalog handle is closed before returning; every pointer
    // passed outlives its call.
    unsafe {
        // A verify-context provider is enough for building a catalog.
        let mut prov: usize = 0;
        if CryptAcquireContextW(
            &mut prov,
            core::ptr::null(),
            core::ptr::null(),
            PROV_RSA_FULL,
            CRYPT_VERIFYCONTEXT,
        ) == 0
        {
            return Err(last_err("CryptAcquireContextW(cat)"));
        }

        let hcat = CryptCATOpen(cat_w.as_ptr(), CRYPTCAT_OPEN_CREATENEW, prov, 0, 0);
        if hcat == INVALID_HANDLE_VALUE || hcat.is_null() {
            CryptReleaseContext(prov, 0);
            return Err(last_err("CryptCATOpen"));
        }

        let result = (|| {
            put_cat_attr(hcat, "HWID1", &hwid_w)?;
            put_cat_attr(hcat, "OS", &os_w)?;
            add_inf_member(hcat, inf_name, &hash)?;
            if CryptCATPersistStore(hcat) == 0 {
                return Err(last_err("CryptCATPersistStore"));
            }
            Ok(())
        })();

        CryptCATClose(hcat);
        CryptReleaseContext(prov, 0);
        result
    }
}

/// Compute the Authenticode SHA-1 hash of a file (the hash a catalog member
/// carries), via `CryptCATAdminCalcHashFromFileHandle`.
fn authenticode_sha1(path: &std::path::Path) -> Result<[u8; SHA1_LEN]> {
    let file = std::fs::File::open(path).map_err(|e| {
        Error::new(
            ErrorCode::Driver,
            format!("cannot open INF for hashing: {e}"),
        )
    })?;
    let handle = file.as_raw_handle() as HANDLE;
    let mut hash = [0u8; SHA1_LEN];
    let mut len = SHA1_LEN as u32;
    // SAFETY: FFI. `handle` is valid for the duration (file kept alive), and the
    // hash buffer matches the length passed.
    let ok = unsafe { CryptCATAdminCalcHashFromFileHandle(handle, &mut len, hash.as_mut_ptr(), 0) };
    if ok == 0 {
        return Err(last_err("CryptCATAdminCalcHashFromFileHandle"));
    }
    Ok(hash)
}

/// Add a catalog-level attribute (`HWID1`, `OS`). `value_w` is a NUL-terminated
/// UTF-16 string; the catalog stores its raw bytes.
///
/// # Safety
/// `hcat` must be an open catalog handle.
unsafe fn put_cat_attr(hcat: HANDLE, tag: &str, value_w: &[u16]) -> Result<()> {
    let tag_w = wide(tag);
    let bytes = core::mem::size_of_val(value_w) as u32;
    let attr = CryptCATPutCatAttrInfo(
        hcat,
        tag_w.as_ptr(),
        CRYPTCAT_ATTR_AUTHENTICATED | CRYPTCAT_ATTR_NAMEASCII | CRYPTCAT_ATTR_DATAASCII,
        bytes,
        value_w.as_ptr() as *mut u8,
    );
    if attr.is_null() {
        return Err(last_err(&format!("CryptCATPutCatAttrInfo({tag})")));
    }
    Ok(())
}

/// Add the INF file as a catalog member: an `<<<Obsolete>>>` SPC link + the
/// SHA-1 hash wrapped in `SIP_INDIRECT_DATA`, plus the `File`/`OSAttr`
/// attributes. Mirrors libwdi `AddFileHash` for the non-PE (INF) branch.
///
/// # Safety
/// `hcat` must be an open catalog handle.
unsafe fn add_inf_member(hcat: HANDLE, inf_name: &str, hash: &[u8; SHA1_LEN]) -> Result<()> {
    // The reference tag is the uppercase hex of the SHA-1 hash.
    let mut hex = String::with_capacity(2 * SHA1_LEN);
    for b in hash {
        hex.push_str(&format!("{b:02X}"));
    }
    let tag_w = wide(&hex);

    // Encode the "<<<Obsolete>>>" file link (CAB/INF variant).
    let obsolete = wide("<<<Obsolete>>>");
    let mut link = SpcLink {
        dwLinkChoice: SPC_FILE_LINK_CHOICE,
        _pad: 0,
        pwsz: obsolete.as_ptr(),
        _union_tail: [0u8; 24],
    };
    let mut enc_len = 0u32;
    if CryptEncodeObject(
        X509_ASN_ENCODING,
        OID_SPC_CAB_DATA.as_ptr(),
        (&mut link as *mut SpcLink).cast(),
        core::ptr::null_mut(),
        &mut enc_len,
    ) == 0
    {
        return Err(last_err("CryptEncodeObject(SPC link, size)"));
    }
    let mut encoded = vec![0u8; enc_len as usize];
    if CryptEncodeObject(
        X509_ASN_ENCODING,
        OID_SPC_CAB_DATA.as_ptr(),
        (&mut link as *mut SpcLink).cast(),
        encoded.as_mut_ptr(),
        &mut enc_len,
    ) == 0
    {
        return Err(last_err("CryptEncodeObject(SPC link)"));
    }

    let mut hash_buf = *hash;
    let mut sip = SIP_INDIRECT_DATA {
        Data: CRYPT_ATTRIBUTE_TYPE_VALUE {
            pszObjId: OID_SPC_CAB_DATA.as_ptr() as *mut u8,
            Value: CRYPT_INTEGER_BLOB {
                cbData: enc_len,
                pbData: encoded.as_mut_ptr(),
            },
        },
        DigestAlgorithm: CRYPT_ALGORITHM_IDENTIFIER {
            pszObjId: OID_OIWSEC_SHA1.as_ptr() as *mut u8,
            Parameters: CRYPT_INTEGER_BLOB {
                cbData: 0,
                pbData: core::ptr::null_mut(),
            },
        },
        Digest: CRYPT_INTEGER_BLOB {
            cbData: SHA1_LEN as u32,
            pbData: hash_buf.as_mut_ptr(),
        },
    };

    let mut guid = INF_SUBJECT_GUID;
    let member = CryptCATPutMemberInfo(
        hcat,
        core::ptr::null(),
        tag_w.as_ptr(),
        &mut guid,
        0x200,
        core::mem::size_of::<SIP_INDIRECT_DATA>() as u32,
        (&mut sip as *mut SIP_INDIRECT_DATA).cast(),
    );
    if member.is_null() {
        return Err(last_err("CryptCATPutMemberInfo"));
    }

    let name_w = wide(&inf_name.to_ascii_lowercase());
    put_member_attr(hcat, member, "File", &name_w)?;
    let osattr_w = wide(MEMBER_OSATTR);
    put_member_attr(hcat, member, "OSAttr", &osattr_w)?;
    Ok(())
}

/// # Safety
/// `hcat`/`member` must be a live catalog handle and one of its members.
unsafe fn put_member_attr(
    hcat: HANDLE,
    member: *mut windows_sys::Win32::Security::Cryptography::Catalog::CRYPTCATMEMBER,
    tag: &str,
    value_w: &[u16],
) -> Result<()> {
    let tag_w = wide(tag);
    let attr = CryptCATPutAttrInfo(
        hcat,
        member,
        tag_w.as_ptr(),
        CRYPTCAT_ATTR_AUTHENTICATED | CRYPTCAT_ATTR_NAMEASCII | CRYPTCAT_ATTR_DATAASCII,
        core::mem::size_of_val(value_w) as u32,
        value_w.as_ptr() as *mut u8,
    );
    if attr.is_null() {
        return Err(last_err(&format!("CryptCATPutAttrInfo({tag})")));
    }
    Ok(())
}

// ------------------------------------------------------------------- signing

/// Sign `file` (the `.cat`) so the package is system-trusted: remove any stale
/// cert with our subject, create a fresh self-signed code-signing cert, trust it
/// in `Root` + `TrustedPublisher`, sign, then destroy the private key. Mirrors
/// libwdi `SelfSignFile`.
pub(crate) fn self_sign_file(file: &std::path::Path) -> Result<()> {
    // Best-effort removal of a previous cert with the same subject.
    let _ = remove_cert_from_store(CERT_SUBJECT, "Root");
    let _ = remove_cert_from_store(CERT_SUBJECT, "TrustedPublisher");

    // SAFETY: FFI. `cert` is freed on every path; all pointers outlive their use.
    unsafe {
        let cert = create_self_signed_cert(CERT_SUBJECT)?;
        let result = (|| {
            add_cert_to_store(cert, "Root")?;
            add_cert_to_store(cert, "TrustedPublisher")?;
            sign_with_cert(file, cert)
        })();
        // Always destroy the private key so it can never be reused, even on error
        // (the public cert may already be trusted).
        let _ = delete_private_key(cert);
        CertFreeCertificateContext(cert);
        result
    }
}

/// Create a self-signed code-signing certificate in a machine key container.
///
/// # Safety
/// Returns a cert context the caller must free with `CertFreeCertificateContext`.
unsafe fn create_self_signed_cert(subject: &str) -> Result<*mut CERT_CONTEXT> {
    // Encode the code-signing-only Enhanced Key Usage extension.
    let mut code_signing = OID_CODE_SIGNING.as_ptr();
    let mut eku = CertEnhKeyUsage {
        cUsageIdentifier: 1,
        rgpszUsageIdentifier: &mut code_signing,
    };
    let mut eku_len = 0u32;
    if CryptEncodeObject(
        X509_ASN_ENCODING,
        X509_ENHANCED_KEY_USAGE,
        (&mut eku as *mut CertEnhKeyUsage).cast(),
        core::ptr::null_mut(),
        &mut eku_len,
    ) == 0
    {
        return Err(last_err("CryptEncodeObject(EKU, size)"));
    }
    let mut eku_buf = vec![0u8; eku_len as usize];
    if CryptEncodeObject(
        X509_ASN_ENCODING,
        X509_ENHANCED_KEY_USAGE,
        (&mut eku as *mut CertEnhKeyUsage).cast(),
        eku_buf.as_mut_ptr(),
        &mut eku_len,
    ) == 0
    {
        return Err(last_err("CryptEncodeObject(EKU)"));
    }
    let mut ext = CERT_EXTENSION {
        pszObjId: OID_ENHANCED_KEY_USAGE.as_ptr() as *mut u8,
        fCritical: 1, // code signing only
        Value: CRYPT_INTEGER_BLOB {
            cbData: eku_len,
            pbData: eku_buf.as_mut_ptr(),
        },
    };
    let mut exts = CERT_EXTENSIONS {
        cExtension: 1,
        rgExtension: &mut ext,
    };

    // Acquire (or create) the machine key container and generate an RSA key.
    let container = wide(KEY_CONTAINER);
    let mut csp: usize = 0;
    if CryptAcquireContextW(
        &mut csp,
        container.as_ptr(),
        core::ptr::null(),
        PROV_RSA_FULL,
        CRYPT_MACHINE_KEYSET | CRYPT_SILENT,
    ) == 0
    {
        let e = GetLastError();
        if e != NTE_BAD_KEYSET
            || CryptAcquireContextW(
                &mut csp,
                container.as_ptr(),
                core::ptr::null(),
                PROV_RSA_FULL,
                CRYPT_NEWKEYSET | CRYPT_MACHINE_KEYSET | CRYPT_SILENT,
            ) == 0
        {
            return Err(last_err("CryptAcquireContextW(key container)"));
        }
    }
    let mut key: usize = 0;
    // RSA-2048, exportable (upper 16 bits carry the key size).
    if CryptGenKey(
        csp,
        AT_SIGNATURE,
        (2048u32 << 16) | CRYPT_EXPORTABLE,
        &mut key,
    ) == 0
    {
        CryptReleaseContext(csp, 0);
        return Err(last_err("CryptGenKey"));
    }

    let cert = create_cert_with_key(subject, &container, &mut exts);

    CryptDestroyKey(key);
    CryptReleaseContext(csp, 0);
    cert
}

/// Encode the subject and call `CertCreateSelfSignCertificate` bound to the
/// machine key container. Split out so the CSP/key are always released.
///
/// # Safety
/// `exts` must reference live extension buffers for the duration of the call.
unsafe fn create_cert_with_key(
    subject: &str,
    container: &[u16],
    exts: &mut CERT_EXTENSIONS,
) -> Result<*mut CERT_CONTEXT> {
    let subject_w = wide(subject);
    let mut name_len = 0u32;
    if CertStrToNameW(
        X509_ASN_ENCODING,
        subject_w.as_ptr(),
        CERT_X500_NAME_STR,
        core::ptr::null_mut(),
        core::ptr::null_mut(),
        &mut name_len,
        core::ptr::null_mut(),
    ) == 0
    {
        return Err(last_err("CertStrToNameW(size)"));
    }
    let mut name_buf = vec![0u8; name_len as usize];
    if CertStrToNameW(
        X509_ASN_ENCODING,
        subject_w.as_ptr(),
        CERT_X500_NAME_STR,
        core::ptr::null_mut(),
        name_buf.as_mut_ptr(),
        &mut name_len,
        core::ptr::null_mut(),
    ) == 0
    {
        return Err(last_err("CertStrToNameW"));
    }
    let subject_blob = CRYPT_INTEGER_BLOB {
        cbData: name_len,
        pbData: name_buf.as_mut_ptr(),
    };

    let key_prov = CRYPT_KEY_PROV_INFO {
        pwszContainerName: container.as_ptr() as *mut u16,
        pwszProvName: core::ptr::null_mut(),
        dwProvType: PROV_RSA_FULL,
        dwFlags: CRYPT_MACHINE_KEYSET,
        cProvParam: 0,
        rgProvParam: core::ptr::null_mut(),
        dwKeySpec: AT_SIGNATURE,
    };
    let sig_algo = CRYPT_ALGORITHM_IDENTIFIER {
        pszObjId: OID_RSA_SHA256RSA.as_ptr() as *mut u8,
        Parameters: CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: core::ptr::null_mut(),
        },
    };
    // Fixed far-future expiry (matches libwdi's approach of a static end date).
    let end = windows_sys::Win32::Foundation::SYSTEMTIME {
        wYear: 2039,
        wMonth: 1,
        wDayOfWeek: 0,
        wDay: 1,
        wHour: 0,
        wMinute: 0,
        wSecond: 0,
        wMilliseconds: 0,
    };

    let cert = CertCreateSelfSignCertificate(
        0,
        &subject_blob,
        0,
        &key_prov,
        &sig_algo,
        core::ptr::null(),
        &end,
        exts,
    );
    if cert.is_null() {
        return Err(last_err("CertCreateSelfSignCertificate"));
    }
    Ok(cert)
}

/// Add `cert` to the LocalMachine `store_name` store with a friendly name.
///
/// # Safety
/// `cert` must be a valid cert context.
unsafe fn add_cert_to_store(cert: *mut CERT_CONTEXT, store_name: &str) -> Result<()> {
    let store = open_machine_store(store_name)?;
    let friendly = wide("numanager");
    let name_blob = CRYPT_INTEGER_BLOB {
        cbData: core::mem::size_of_val(friendly.as_slice()) as u32,
        pbData: friendly.as_ptr() as *mut u8,
    };
    CertSetCertificateContextProperty(
        cert,
        CERT_FRIENDLY_NAME_PROP_ID,
        0,
        (&name_blob as *const CRYPT_INTEGER_BLOB).cast(),
    );
    let ok = CertAddCertificateContextToStore(
        store,
        cert,
        CERT_STORE_ADD_REPLACE_EXISTING,
        core::ptr::null_mut(),
    );
    CertCloseStore(store, 0);
    if ok == 0 {
        return Err(last_err(&format!(
            "CertAddCertificateContextToStore({store_name})"
        )));
    }
    Ok(())
}

/// Sign `file` with `cert` using `SignerSignEx` (SHA-256 Authenticode).
///
/// # Safety
/// `cert` must be a valid cert context with an accessible private key.
unsafe fn sign_with_cert(file: &std::path::Path, cert: *mut CERT_CONTEXT) -> Result<()> {
    let file_w = wide(&file.to_string_lossy());
    let mut file_info = SIGNER_FILE_INFO {
        cbSize: core::mem::size_of::<SIGNER_FILE_INFO>() as u32,
        pwszFileName: file_w.as_ptr(),
        hFile: core::ptr::null_mut(),
    };
    let mut index = 0u32;
    let subject_info = SIGNER_SUBJECT_INFO {
        cbSize: core::mem::size_of::<SIGNER_SUBJECT_INFO>() as u32,
        pdwIndex: &mut index,
        dwSubjectChoice: SIGNER_SUBJECT_FILE,
        Anonymous: SIGNER_SUBJECT_INFO_0 {
            pSignerFileInfo: &mut file_info,
        },
    };
    let mut store_info = SIGNER_CERT_STORE_INFO {
        cbSize: core::mem::size_of::<SIGNER_CERT_STORE_INFO>() as u32,
        pSigningCert: cert,
        dwCertPolicy: SIGNER_CERT_POLICY_CHAIN,
        hCertStore: core::ptr::null_mut(),
    };
    let signer_cert = SIGNER_CERT {
        cbSize: core::mem::size_of::<SIGNER_CERT>() as u32,
        dwCertChoice: SIGNER_CERT_STORE,
        Anonymous: SIGNER_CERT_0 {
            pCertStoreInfo: &mut store_info,
        },
        hwnd: core::ptr::null_mut(),
    };
    let sig_info = SIGNER_SIGNATURE_INFO {
        cbSize: core::mem::size_of::<SIGNER_SIGNATURE_INFO>() as u32,
        algidHash: CALG_SHA_256,
        dwAttrChoice: SIGNER_NO_ATTR,
        Anonymous: SIGNER_SIGNATURE_INFO_0 {
            pAttrAuthcode: core::ptr::null_mut(),
        },
        psAuthenticated: core::ptr::null_mut(),
        psUnauthenticated: core::ptr::null_mut(),
    };

    let mut signer_ctx: *mut SIGNER_CONTEXT = core::ptr::null_mut();
    let hr = SignerSignEx(
        0,
        &subject_info,
        &signer_cert,
        &sig_info,
        core::ptr::null(),
        core::ptr::null(),
        core::ptr::null(),
        core::ptr::null(),
        &mut signer_ctx,
    );
    if !signer_ctx.is_null() {
        SignerFreeSignerContext(signer_ctx);
    }
    if hr != 0 {
        return Err(Error::new(
            ErrorCode::Driver,
            format!("SignerSignEx failed (HRESULT=0x{hr:08x})"),
        ));
    }
    Ok(())
}

/// Destroy the private key for `cert` (so the trusted cert cannot be reused to
/// sign anything else), then re-import the cert into the stores so the OS no
/// longer reports an associated private key. Mirrors libwdi `DeletePrivateKey`.
///
/// # Safety
/// `cert` must be a valid cert context.
unsafe fn delete_private_key(cert: *mut CERT_CONTEXT) -> Result<()> {
    let container = wide(KEY_CONTAINER);
    let mut csp: usize = 0;
    // Acquiring with CRYPT_DELETEKEYSET destroys the key container.
    let _ = CryptAcquireContextW(
        &mut csp,
        container.as_ptr(),
        core::ptr::null(),
        PROV_RSA_FULL,
        CRYPT_MACHINE_KEYSET | CRYPT_SILENT | CRYPT_DELETEKEYSET,
    );

    for store_name in ["Root", "TrustedPublisher"] {
        let store = match open_machine_store(store_name) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let mut updated: *mut CERT_CONTEXT = core::ptr::null_mut();
        CertAddEncodedCertificateToStore(
            store,
            X509_ASN_ENCODING,
            (*cert).pbCertEncoded,
            (*cert).cbCertEncoded,
            CERT_STORE_ADD_REPLACE_EXISTING,
            &mut updated,
        );
        if !updated.is_null() {
            CertFreeCertificateContext(updated);
        }
        CertCloseStore(store, 0);
    }
    Ok(())
}

/// Delete every cert with subject `subject` from the LocalMachine `store_name`
/// store. Mirrors libwdi `RemoveCertFromStore`.
pub(crate) fn remove_cert_from_store(subject: &str, store_name: &str) -> Result<()> {
    let subject_w = wide(subject);
    // SAFETY: FFI. The store is closed before returning.
    unsafe {
        let store = open_machine_store(store_name)?;

        let mut blob_len = 0u32;
        if CertStrToNameW(
            X509_ASN_ENCODING,
            subject_w.as_ptr(),
            CERT_X500_NAME_STR,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            &mut blob_len,
            core::ptr::null_mut(),
        ) == 0
        {
            CertCloseStore(store, 0);
            return Err(last_err("CertStrToNameW(size)"));
        }
        let mut blob_buf = vec![0u8; blob_len as usize];
        if CertStrToNameW(
            X509_ASN_ENCODING,
            subject_w.as_ptr(),
            CERT_X500_NAME_STR,
            core::ptr::null_mut(),
            blob_buf.as_mut_ptr(),
            &mut blob_len,
            core::ptr::null_mut(),
        ) == 0
        {
            CertCloseStore(store, 0);
            return Err(last_err("CertStrToNameW"));
        }
        let name_blob = CRYPT_INTEGER_BLOB {
            cbData: blob_len,
            pbData: blob_buf.as_mut_ptr(),
        };

        // Each delete frees the context, so re-find from NULL every time.
        loop {
            let found = CertFindCertificateInStore(
                store,
                X509_ASN_ENCODING,
                0,
                CERT_FIND_SUBJECT_NAME,
                (&name_blob as *const CRYPT_INTEGER_BLOB).cast(),
                core::ptr::null_mut(),
            );
            if found.is_null() {
                break;
            }
            if CertDeleteCertificateFromStore(found) == 0 {
                CertCloseStore(store, 0);
                return Err(last_err(&format!(
                    "CertDeleteCertificateFromStore({store_name})"
                )));
            }
        }
        CertCloseStore(store, 0);
        Ok(())
    }
}

/// Open a LocalMachine system certificate store by name.
///
/// # Safety
/// The returned store must be closed with `CertCloseStore`.
unsafe fn open_machine_store(store_name: &str) -> Result<HCERTSTORE> {
    let name_w = wide(store_name);
    let store = CertOpenStore(
        CERT_STORE_PROV_SYSTEM_W,
        X509_ASN_ENCODING,
        0,
        CERT_SYSTEM_STORE_LOCAL_MACHINE,
        name_w.as_ptr().cast(),
    );
    if store.is_null() {
        return Err(last_err(&format!("CertOpenStore({store_name})")));
    }
    Ok(store)
}
