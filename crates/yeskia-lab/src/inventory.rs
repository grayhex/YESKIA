//! Инвентаризация стенда и «попытка идентификации» (USB enumeration → identify attempt) средствами
//! YESKIA: read-only, без записи на устройство. Результат — заготовка отчёта; поля, которые нельзя
//! узнать автоматически (модель, RM, прошивка, версия NPC), помечены `TODO` и должны быть
//! заполнены человеком до прохождения `validate`.

use crate::report::*;
use yeskia_discovery::doctor::{analyze_with_refs, DoctorState};
use yeskia_discovery::model::{CollectOptions, Snapshot};

fn state_label(s: DoctorState) -> &'static str {
    match s {
        DoctorState::WpdOnly => "WPD/MTP",
        DoctorState::MassStorage => "Mass Storage",
        DoctorState::ComPresent => "COM (порт есть)",
        DoctorState::ComBusy => "COM (порт занят)",
        DoctorState::DriverMissing => "драйвер отсутствует",
        DoctorState::DriverError => "ошибка драйвера",
        DoctorState::UnknownMode => "режим не определён",
    }
}

/// Строит заготовку отчёта по снимку; `identify` — результат read-only попытки идентификации.
pub fn skeleton_from_snapshot(
    snap: &Snapshot,
    created: &str,
    identify: impl Fn(&str) -> Result<String, String>,
) -> ExperimentReport {
    let (doctor, refs) = analyze_with_refs(snap, 0);
    let dev = doctor.devices.iter().find(|d| d.nokia_vendor_hint);
    let rf = dev.and_then(|d| refs.iter().find(|r| r.alias == d.alias));

    let mut findings = Vec::new();
    let mut known = Vec::new();
    let mut unknown = vec![
        "Модель и ревизия телефона подтверждаются только идентификацией по протоколу".to_string(),
        "Поведение PC Suite-режима и PIM не исследовалось".to_string(),
    ];
    let (usb_mode, vid_pid, driver, channel) = match dev {
        Some(d) => {
            findings.push(FindingEntry {
                statement: format!(
                    "USB-устройство {}:{} видно, режим: {}",
                    d.vid,
                    d.pid,
                    state_label(d.state)
                ),
                evidence: Evidence::Observed,
                adapter_ref: None,
            });
            known.push(format!(
                "USB enumeration: {}:{}, режим {}",
                d.vid,
                d.pid,
                state_label(d.state)
            ));
            if let (Some(r), true) = (
                rf,
                d.protocol != yeskia_discovery::doctor::ProtocolStatus::NotProbed,
            ) {
                match r.wpd_device_id.as_deref().map(&identify) {
                    Some(Ok(model)) => {
                        findings.push(FindingEntry {
                            statement: format!(
                                "Попытка идентификации через WPD вернула модель: {model}"
                            ),
                            evidence: Evidence::Observed,
                            adapter_ref: None,
                        });
                        known.push("WPD отвечает на чтение свойств устройства (read-only)".into());
                    }
                    Some(Err(code)) => {
                        findings.push(FindingEntry {
                            statement: format!(
                                "Попытка идентификации через WPD не удалась: {code}"
                            ),
                            evidence: Evidence::Observed,
                            adapter_ref: None,
                        });
                        unknown.push("Причина отказа идентификации через WPD".into());
                    }
                    None => {}
                }
            }
            (
                state_label(d.state).to_string(),
                format!("{}:{}", d.vid, d.pid),
                [
                    d.driver.class.clone(),
                    d.driver.service.clone(),
                    d.driver.version.clone(),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" / "),
                if matches!(d.state, DoctorState::ComPresent | DoctorState::ComBusy) {
                    Channel::PcSuiteSerial
                } else {
                    Channel::Wpd
                },
            )
        }
        None => {
            findings.push(FindingEntry {
                statement: "Устройство с VID 0421 не обнаружено".into(),
                evidence: Evidence::Observed,
                adapter_ref: None,
            });
            known.push("USB-устройств Nokia в снимке нет".into());
            ("TODO".into(), "TODO".into(), "TODO".into(), Channel::Wpd)
        }
    };

    ExperimentReport {
        schema: REPORT_SCHEMA.into(),
        id: "EXP-0000".into(),
        title: "TODO: название эксперимента".into(),
        created: created.into(),
        synthetic: false,
        setup: Setup {
            os: format!("{} {}", doctor.platform.os, doctor.platform.build)
                .trim()
                .to_string(),
            phone_model: "TODO".into(),
            phone_rm: None,
            firmware: "TODO".into(),
            driver: if driver.is_empty() {
                "TODO".into()
            } else {
                driver
            },
            usb_mode,
            npc_version: None,
            usb_vid_pid: vid_pid,
            usb_interface: None,
        },
        test_data: TestData {
            separate_test_data: false,
            user_data_backed_up: false,
        },
        action: "enumerate → identify attempt".into(),
        channel,
        capture: Capture {
            tool: "TODO".into(),
            filter: None,
            raw_stored_locally: true,
            raw_committed: false,
            retention_days: 14,
        },
        redaction: Redaction {
            tool: "yeskia-lab inventory".into(),
            categories: vec!["imei".into(), "serial".into(), "usb_instance_id".into()],
            reviewed_by_human: false,
        },
        findings,
        known,
        unknown,
        next_experiment: "TODO: следующий конкретный эксперимент".into(),
    }
}

/// Текущая система: снимок через `yeskia-discovery` + реальная read-only идентификация WPD.
pub fn collect_now(created: &str) -> ExperimentReport {
    let source = yeskia_discovery::system_source();
    let snap = source
        .collect(&CollectOptions::default())
        .unwrap_or_default();
    skeleton_from_snapshot(&snap, created, |wpd_id| {
        source
            .wpd_identity(wpd_id)
            .map(|i| i.model.unwrap_or_else(|| "модель не сообщена".into()))
            .map_err(|e| format!("этап {}, код {:#010X}", e.stage, e.code.unwrap_or(0)))
    })
}
