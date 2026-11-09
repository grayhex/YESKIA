# Шаблон отчёта эксперимента (`yeskia.lab.report/1`)

Файл отчёта — JSON (`docs/protocol/cases/<ID>.report.json`). Ниже — пояснение полей; пустой каркас с реального
стенда создаёт `yeskia-lab inventory --out …`.

| Поле | Содержание |
| --- | --- |
| `schema` | всегда `yeskia.lab.report/1` |
| `id`, `title`, `created` | `EXP-0001`, название, дата `YYYY-MM-DD` |
| `synthetic` | `true` только для вымышленных данных (демо/тесты) |
| `setup` | `os`, `phone_model`, `phone_rm`, `firmware`, `driver`, `usb_mode`, `npc_version`, `usb_vid_pid`, `usb_interface` — **без IMEI/серийников** |
| `test_data` | `separate_test_data` и `user_data_backed_up` — оба `true` |
| `action`, `channel` | одно действие и один канал: `wpd` / `pc_suite_serial` / `usb_raw` / `bluetooth` |
| `capture` | `tool`, `filter`, `raw_stored_locally`, `raw_committed` (**всегда false**), `retention_days` (1–90) |
| `redaction` | `tool`, `categories`, `reviewed_by_human` (для реальных данных — `true`) |
| `findings[]` | `statement` + `evidence` (`observed` / `inferred` / `hypothesis` / `adapter-verified`) + `adapter_ref` |
| `known[]`, `unknown[]` | что известно / что неизвестно (оба обязательны) |
| `next_experiment` | конкретный следующий эксперимент |

Правила проверки (`yeskia-lab validate`): нет плейсхолдеров `TODO`; нет приватных данных; утверждения о
«поддержке» только при `adapter-verified`; синтетический отчёт не может быть `adapter-verified`.
