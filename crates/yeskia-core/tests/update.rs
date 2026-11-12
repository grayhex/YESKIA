//! Обновление из файла (issue #11): проверка манифеста, хеша, подписи; запуск только по подтверждению
//! и не во время записи на телефон.

use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use yeskia_core::app::AppCore;
use yeskia_core::device::DeviceId;
use yeskia_core::error::ErrorCode;
use yeskia_core::mock::MOCK_DEVICE_ID;
use yeskia_core::operations::{OperationRequest, OverwritePolicy};
use yeskia_core::update::{self, Trust};

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "yeskia-upd-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

struct Pkg {
    dir: PathBuf,
    installer: String,
    manifest: String,
}

// Test an actual future version after every release bump, rather than a fixed 0.2.0.
fn newer_version() -> String {
    let patch = env!("CARGO_PKG_VERSION_PATCH").parse::<u64>().unwrap() + 1;
    format!(
        "{}.{}.{patch}",
        env!("CARGO_PKG_VERSION_MAJOR"),
        env!("CARGO_PKG_VERSION_MINOR")
    )
}

fn package(tag: &str, version: &str) -> Pkg {
    let dir = tmp(tag);
    let data: Vec<u8> = (0..50_000u32).map(|i| (i % 251) as u8).collect();
    let name = format!("YESKIA_{version}_x64-setup.exe");
    std::fs::write(dir.join(&name), &data).unwrap();
    let m = format!(
        "{{\"version\":\"{version}\",\"file\":\"{name}\",\"sha256\":\"{}\",\"size\":{},\"notes\":\"Заметки\"}}",
        yeskia_core::fsutil::sha256_hex(&data),
        data.len()
    );
    std::fs::write(dir.join("manifest.json"), m).unwrap();
    Pkg {
        installer: dir.join(&name).display().to_string(),
        manifest: dir.join("manifest.json").display().to_string(),
        dir,
    }
}

fn keypair() -> (Ed25519KeyPair, [u8; 32]) {
    let doc = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    let kp = Ed25519KeyPair::from_pkcs8(doc.as_ref()).unwrap();
    let mut pk = [0u8; 32];
    pk.copy_from_slice(kp.public_key().as_ref());
    (kp, pk)
}

fn sign(p: &Pkg, kp: &Ed25519KeyPair) -> String {
    let sig = hex(kp.sign(&std::fs::read(&p.manifest).unwrap()).as_ref());
    let path = p.dir.join("manifest.sig");
    std::fs::write(&path, sig).unwrap();
    path.display().to_string()
}

#[test]
fn without_a_configured_key_the_source_is_unsigned_and_says_so() {
    assert!(
        update::TRUSTED_KEYS.is_empty(),
        "ключ издателя ещё не настроен владельцем"
    );
    let core = AppCore::in_memory();
    let p = package("unsigned", &newer_version());
    let c = core.update_check(&p.installer, &p.manifest, None).unwrap();
    assert_eq!(c.current, env!("CARGO_PKG_VERSION"));
    assert_eq!(c.version, newer_version());
    assert_eq!(c.trust, Trust::UnsignedSource);
    assert!(c.limitations.iter().any(|l| l.contains("SmartScreen")));
    assert!(c
        .limitations
        .iter()
        .any(|l| l.contains("ничего не скачивает")));
    assert_eq!(c.notes, "Заметки");
}

#[test]
fn a_valid_signature_makes_the_source_signed_and_a_forged_one_is_rejected() {
    let (kp, pk) = keypair();
    let core = AppCore::in_memory().with_update_keys(vec![pk]);
    let p = package("signed", &newer_version());
    let sig = sign(&p, &kp);
    let c = core
        .update_check(&p.installer, &p.manifest, Some(&sig))
        .unwrap();
    assert_eq!(c.trust, Trust::Signed);
    assert!(!c.limitations.iter().any(|l| l.contains("SmartScreen")));
    // манифест изменён после подписи
    let m = std::fs::read_to_string(&p.manifest)
        .unwrap()
        .replace("Заметки", "Другое");
    std::fs::write(&p.manifest, m).unwrap();
    let e = core
        .update_check(&p.installer, &p.manifest, Some(&sig))
        .unwrap_err();
    assert_eq!(e.code(), ErrorCode::VerificationFailed);
    // подпись чужим ключом
    let (kp2, _) = keypair();
    let p2 = package("signed2", &newer_version());
    let sig2 = sign(&p2, &kp2);
    assert_eq!(
        core.update_check(&p2.installer, &p2.manifest, Some(&sig2))
            .unwrap_err()
            .code(),
        ErrorCode::VerificationFailed
    );
    // мусор вместо подписи
    let junk = p2.dir.join("junk.sig");
    std::fs::write(&junk, "not hex").unwrap();
    assert!(core
        .update_check(
            &p2.installer,
            &p2.manifest,
            Some(&junk.display().to_string())
        )
        .is_err());
    // ключ настроен, но подписи нет → неподписанный источник (не «подписан»)
    let c = core.update_check(&p.installer, &p.manifest, None);
    assert_eq!(c.unwrap().trust, Trust::UnsignedSource);
}

#[test]
fn downgrades_replays_and_mismatches_are_refused() {
    let core = AppCore::in_memory();
    for v in [env!("CARGO_PKG_VERSION"), "0.0.9"] {
        let p = package("down", v);
        let e = core
            .update_check(&p.installer, &p.manifest, None)
            .unwrap_err();
        assert_eq!(e.code(), ErrorCode::InvalidArgument, "{v}");
    }
    let p = package("hash", &newer_version());
    // файл подменён после выпуска манифеста
    let mut data = std::fs::read(&p.installer).unwrap();
    data[10] ^= 1;
    std::fs::write(&p.installer, data).unwrap();
    assert_eq!(
        core.update_check(&p.installer, &p.manifest, None)
            .unwrap_err()
            .code(),
        ErrorCode::VerificationFailed
    );
    // другое имя файла
    let p = package("name", &newer_version());
    let other = p.dir.join("renamed-setup.exe");
    std::fs::copy(&p.installer, &other).unwrap();
    assert!(core
        .update_check(&other.display().to_string(), &p.manifest, None)
        .is_err());
    // размер в манифесте не совпадает
    let m = std::fs::read_to_string(&p.manifest)
        .unwrap()
        .replace("\"size\":50000", "\"size\":49999");
    std::fs::write(&p.manifest, m).unwrap();
    assert!(core.update_check(&p.installer, &p.manifest, None).is_err());
    // не установщик, мусорный манифест, относительные пути
    let p = package("junk", &newer_version());
    std::fs::write(&p.manifest, "{}").unwrap();
    assert!(core.update_check(&p.installer, &p.manifest, None).is_err());
    assert!(core.update_check("rel.exe", "rel.json", None).is_err());
    let big = p.dir.join("big.json");
    std::fs::write(&big, vec![b' '; 70_000]).unwrap();
    assert_eq!(
        core.update_check(&p.installer, &big.display().to_string(), None)
            .unwrap_err()
            .code(),
        ErrorCode::LimitExceeded
    );
}

#[test]
fn launch_needs_confirmation_and_separate_consent_for_unsigned() {
    let launched = Arc::new(AtomicUsize::new(0));
    let l2 = Arc::clone(&launched);
    let core = AppCore::in_memory().with_launcher(move |_| {
        l2.fetch_add(1, Ordering::SeqCst);
        Ok(())
    });
    let p = package("launch", &newer_version());
    assert!(
        core.update_launch(&p.installer, &p.manifest, None, false, true)
            .is_err(),
        "без подтверждения"
    );
    assert!(
        core.update_launch(&p.installer, &p.manifest, None, true, false)
            .is_err(),
        "неподписанный без согласия"
    );
    assert_eq!(launched.load(Ordering::SeqCst), 0);
    core.update_launch(&p.installer, &p.manifest, None, true, true)
        .unwrap();
    assert_eq!(launched.load(Ordering::SeqCst), 1);
    // файл подменили между проверкой и запуском: повторная проверка отказывает
    let mut data = std::fs::read(&p.installer).unwrap();
    data[0] ^= 0xFF;
    std::fs::write(&p.installer, data).unwrap();
    assert!(core
        .update_launch(&p.installer, &p.manifest, None, true, true)
        .is_err());
    assert_eq!(launched.load(Ordering::SeqCst), 1);
}

#[test]
fn signed_source_needs_no_unsigned_consent() {
    let (kp, pk) = keypair();
    let n = Arc::new(AtomicUsize::new(0));
    let n2 = Arc::clone(&n);
    let core = AppCore::in_memory()
        .with_update_keys(vec![pk])
        .with_launcher(move |_| {
            n2.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });
    let p = package("sl", &newer_version());
    let sig = sign(&p, &kp);
    core.update_launch(&p.installer, &p.manifest, Some(&sig), true, false)
        .unwrap();
    assert_eq!(n.load(Ordering::SeqCst), 1);
}

#[test]
fn update_is_postponed_while_a_phone_write_is_in_progress() {
    let launched = Arc::new(AtomicUsize::new(0));
    let l2 = Arc::clone(&launched);
    let core = Arc::new(AppCore::in_memory().with_launcher(move |_| {
        l2.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }));
    let id = DeviceId::new(MOCK_DEVICE_ID);
    core.engine.connect(&id).unwrap();
    assert!(core.update_blockers().is_empty());
    core.mock_handle().set_write_profile(10, 40); // запись идёт ~400 мс
    let op = core
        .start_operation(
            &id,
            OperationRequest::FilesWrite {
                path: "E:/slow.bin".into(),
                data: vec![7; 1000],
                overwrite: OverwritePolicy::Never,
            },
        )
        .unwrap();
    // операция принята, но ещё не завершена
    assert_eq!(core.update_blockers(), vec!["device-operation"]);
    let p = package("busy", &newer_version());
    let err = core
        .update_launch(&p.installer, &p.manifest, None, true, true)
        .unwrap_err();
    assert_eq!(err.code(), ErrorCode::OperationFailed);
    assert_eq!(
        launched.load(Ordering::SeqCst),
        0,
        "установщик не запускался"
    );
    let c2 = Arc::clone(&core);
    let t = std::thread::spawn(move || c2.await_operation(&op).unwrap());
    let r = t.join().unwrap();
    assert_eq!(r.outcome, yeskia_core::operations::Outcome::Success);
    assert!(
        core.update_blockers().is_empty(),
        "после завершения запись не блокирует"
    );
    core.update_launch(&p.installer, &p.manifest, None, true, true)
        .unwrap();
    assert_eq!(launched.load(Ordering::SeqCst), 1);
}

#[test]
fn a_running_host_job_blocks_the_update() {
    let core = AppCore::in_memory();
    let src = tmp("hj-src");
    std::fs::write(src.join("a.txt"), vec![1u8; 1000]).unwrap();
    let id = core
        .start_host_job(yeskia_core::app::HostJobRequest::BackupCreateLocal {
            source: src.display().to_string(),
            label: "u".into(),
            exclude: vec![],
        })
        .unwrap();
    assert_eq!(core.update_blockers(), vec!["host-job"]);
    let r = core.await_host_job(&id).unwrap();
    assert_eq!(r.outcome, yeskia_core::operations::Outcome::Success);
    assert!(core.update_blockers().is_empty());
}

#[test]
fn node_signature_verifies_in_rust() {
    // манифест подписан scripts/release.mjs (Node, Ed25519): совместимость с проверкой приложения
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/update");
    let manifest = std::fs::read(dir.join("manifest.json")).unwrap();
    let sig = std::fs::read_to_string(dir.join("manifest.sig")).unwrap();
    let pk_hex = std::fs::read_to_string(dir.join("public-key.hex")).unwrap();
    let pk: Vec<u8> = (0..32)
        .map(|i| u8::from_str_radix(&pk_hex.trim()[2 * i..2 * i + 2], 16).unwrap())
        .collect();
    let key: [u8; 32] = pk.try_into().unwrap();
    assert!(update::verify_signature(&manifest, Some(sig.trim()), &[key]).unwrap());
    let mut forged = manifest.clone();
    forged[12] ^= 1;
    assert!(update::verify_signature(&forged, Some(sig.trim()), &[key]).is_err());
    // без ключей подпись не считается проверенной (но и не ошибка)
    assert!(!update::verify_signature(&manifest, Some(sig.trim()), &[]).unwrap());
}
