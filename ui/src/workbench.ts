import tlsManifest from "../../crates/yeskia-core/src/revive/tls_components.json";
import { invoke } from "@tauri-apps/api/core";
export interface Versioned<T> {
  revision: number;
  value: T;
}
export interface ContactValue {
  value: string;
  label: string;
  params: string[];
  components: string[];
}
export interface Contact {
  id: string;
  name: string;
  given: string;
  family: string;
  middle: string;
  prefix: string;
  suffix: string;
  phones: ContactValue[];
  emails: ContactValue[];
  addresses: ContactValue[];
  organisation: string[];
  title: string;
  note: string;
  birthday: string;
  websites: ContactValue[];
  groups: string[];
  unknown: { head: string; value: string }[];
  csv_extra: Record<string, string>;
  source: {
    kind: string;
    native_id: string | null;
    fingerprint: string | null;
    original_id: string | null;
  };
  status: string;
}
export interface Book {
  contacts: Contact[];
}
export interface ImportOptions {
  format: string;
  encoding: string;
  delimiter: string | null;
  mapping: Record<number, string>;
}
export interface Imported {
  book: Book;
  warnings: string[];
  encoding: string;
}
export interface CsvPreview {
  columns: string[];
  sample: string[][];
  rows: number;
  delimiter: string;
  encoding: string;
  mapping: Record<number, string>;
}
export interface Duplicate {
  ids: string[];
  reasons: string[];
}
export interface Change {
  id: string;
  kind: string;
  fields: string[];
}
export interface PhoneSnapshot {
  book: Book;
  previous: Book;
  changes: Change[];
  hash: string;
  source: string;
  read_at_ms: number;
  write_status: string;
}
export interface TlsComponent {
  id: string;
  cached: boolean;
  spec: {
    title: Record<string, string>;
    version: string;
    name: string;
    sha256: string;
    size: number;
    url: string;
    author: string;
    released: string;
    licence: unknown;
    release_notes_text: string;
    source_repository: string;
    homepage: string;
    risk: string;
    install: string;
    depends_on: string[];
  };
  validation: TlsValidation | null;
}
export interface TlsValidation {
  component: string;
  format: string;
  sha256: string;
  size: number;
  package_uid: number | null;
  signed: boolean | null;
  uid_checksum: boolean | null;
  controller_crc: boolean | null;
  data_crc: boolean | null;
  files: string[];
}
export interface Target {
  model: string;
  rm: string;
  firmware: string;
  os: string;
  identity_confirmed: boolean;
}
export interface Applicability {
  status: string;
  basis: string;
  components: string[];
  reasons: string[];
  firmware_verified: boolean;
  device_verified: boolean;
  automatic_apply: boolean;
}
export interface Observation {
  status: string;
  source: string;
  checked_at_ms: number;
}
export interface TlsProgress {
  transferred: Record<string, Observation>;
  components: Record<string, Observation>;
  phone_clock: string | null;
  phone_https: Observation | null;
  snapshot: Observation | null;
}
export interface Profile {
  model: string;
  rm: string;
  firmware: string;
  os: string;
}
export type StepStatus = "not-started" | "manual" | "verified" | "blocked" | "not-supported";
export interface PrepProgress {
  steps: Record<string, { status: StepStatus; source: string; checked_at_ms: number }>;
}
export interface Readiness {
  schema: number;
  bench: string;
  model: string;
  rm: string;
  firmware: string;
  os: string;
  transport: string;
  tls_manifest: string;
  steps: PrepProgress["steps"];
  ready: boolean;
  excluded: string[];
}
export interface PrepView {
  key: string;
  context: {
    fingerprint: string;
    transport: string;
    driver_signature: string;
    mode: string;
    profile: Profile;
  };
  progress: Versioned<PrepProgress>;
}
export interface TlsReport {
  schema: string;
  profile: Profile;
  profile_source: string;
  stack: string;
  hardware_status: string;
  manifest_sha256: string;
  components: TlsComponent[];
  progress: TlsProgress;
  excluded: string[];
}
export interface Calls {
  tls_report: { args: { fingerprint: string; profile: Profile; stack: string }; out: TlsReport };
  tls_clock: {
    args: { fingerprint: string; firmware: string; revision: number; date: string };
    out: Versioned<TlsProgress>;
  };
  contacts_load: { args: object; out: Versioned<Book> };
  contacts_save: { args: { revision: number; book: Book }; out: Versioned<Book> };
  contacts_undo: { args: { revision: number }; out: Versioned<Book> };
  contacts_import: { args: { path: string; options: ImportOptions }; out: Imported };
  contacts_csv_preview: { args: { path: string; options: ImportOptions }; out: CsvPreview };
  contacts_export: { args: { path: string; book: Book; format: string }; out: null };
  contacts_duplicates: { args: { book: Book }; out: Duplicate[] };
  contacts_merge: { args: { book: Book; ids: string[]; primary: string }; out: Contact };
  contacts_diff: { args: { before: Book; after: Book }; out: Change[] };
  tls_state: { args: object; out: TlsComponent[] };
  tls_import: { args: { id: string; path: string }; out: TlsValidation };
  tls_extract: { args: object; out: string };
  tls_applicability: { args: { target: Target; stack: string }; out: Applicability };
  tls_progress: { args: { fingerprint: string; firmware: string }; out: Versioned<TlsProgress> };
  tls_record: {
    args: {
      fingerprint: string;
      firmware: string;
      revision: number;
      item: string;
      status: string;
      confirm: boolean;
    };
    out: Versioned<TlsProgress>;
  };
  prep_profile: { args: object; out: Versioned<Profile> };
  prep_save_profile: { args: { profile: Profile }; out: Versioned<Profile> };
}
export type Request<K extends keyof Calls> = { kind: K } & Calls[K]["args"];
export interface WorkbenchBackend {
  call<K extends keyof Calls>(request: Request<K>): Promise<Calls[K]["out"]>;
  preparation(
    request:
      | { kind: "load"; device_id: string | null }
      | {
          kind: "record";
          device_id: string | null;
          revision: number;
          expected_key: string;
          step: string;
          status: Exclude<StepStatus, "verified">;
        },
  ): Promise<PrepView>;
  exportPreparation(deviceId: string | null): Promise<Readiness>;
  phoneRead(deviceId: string): Promise<PhoneSnapshot>;
  openAuthor(id: string): Promise<void>;
  stageTls(deviceId: string, id: string, target: Target): Promise<string>;
}
export class NativeWorkbench implements WorkbenchBackend {
  call<K extends keyof Calls>(request: Request<K>) {
    return invoke<Calls[K]["out"]>("local_workbench", { request });
  }
  preparation(request: Parameters<WorkbenchBackend["preparation"]>[0]) {
    return invoke<PrepView>("preparation", { request });
  }
  exportPreparation(deviceId: string | null) {
    return invoke<Readiness>("preparation", { request: { kind: "report", device_id: deviceId } });
  }
  phoneRead(deviceId: string) {
    return invoke<PhoneSnapshot>("contacts_phone_read", { deviceId });
  }
  stageTls(deviceId: string, id: string, target: Target) {
    return invoke<string>("tls_stage", { deviceId, id, target, acknowledge: true });
  }
  openAuthor(id: string) {
    return invoke<void>("tls_open_author", { id });
  }
}
export const emptyContact = (): Contact => ({
  id: crypto.randomUUID().replaceAll("-", ""),
  name: "",
  given: "",
  family: "",
  middle: "",
  prefix: "",
  suffix: "",
  phones: [],
  emails: [],
  addresses: [],
  organisation: [],
  title: "",
  note: "",
  birthday: "",
  websites: [],
  groups: [],
  unknown: [],
  csv_extra: {},
  source: { kind: "local", native_id: null, fingerprint: null, original_id: null },
  status: "local",
});
export const defaultProfile: Profile = {
  model: "nokia-e72",
  rm: "RM-530",
  firmware: "054.005",
  os: "9.3",
};
/** Browser preview only. Import parsers and pin validation always run in Rust. */
export class MockWorkbench implements WorkbenchBackend {
  private book: Versioned<Book> = { revision: 0, value: { contacts: [] } };
  private history: Book[] = [];
  private profile = { ...defaultProfile };
  private steps: PrepProgress = { steps: {} };
  async call<K extends keyof Calls>(request: Request<K>): Promise<Calls[K]["out"]> {
    const r = request as Request<keyof Calls> & Record<string, unknown>;
    let value: unknown;
    switch (r.kind) {
      case "contacts_load":
        value = structuredClone(this.book);
        break;
      case "contacts_save":
        if (r.revision !== this.book.revision) throw Error("LOCAL_EDIT_CONFLICT_RELOAD");
        this.history.push(structuredClone(this.book.value));
        this.book = { revision: this.book.revision + 1, value: structuredClone(r.book as Book) };
        value = structuredClone(this.book);
        break;
      case "contacts_undo": {
        if (r.revision !== this.book.revision) throw Error("LOCAL_EDIT_CONFLICT_RELOAD");
        const old = this.history.pop();
        if (!old) throw Error("NOTHING_TO_UNDO");
        this.book = { revision: this.book.revision + 1, value: old };
        value = structuredClone(this.book);
        break;
      }
      case "contacts_duplicates":
        value = [];
        break;
      case "prep_profile":
        value = { revision: 0, value: { ...this.profile } };
        break;
      case "prep_save_profile":
        this.profile = { ...(r.profile as Profile) };
        this.steps = { steps: {} };
        value = { revision: 1, value: this.profile };
        break;
      case "tls_state":
        value = Object.entries(tlsManifest.components).map(([id, spec]) => ({
          id,
          spec,
          cached: false,
          validation: null,
        }));
        break;
      case "tls_applicability":
        value = {
          status: "unsupported",
          basis: "UI MOCK",
          components: [],
          reasons: ["Native platform validation required"],
          firmware_verified: false,
          device_verified: false,
          automatic_apply: false,
        };
        break;
      case "tls_report":
        value = {
          schema: "yeskia.tls-report/1",
          profile: r.profile,
          profile_source: "owner-entered; UI MOCK",
          stack: r.stack,
          hardware_status: "not-tested; UI MOCK",
          manifest_sha256: "UI MOCK",
          components: [],
          progress: {
            transferred: {},
            components: {},
            phone_clock: null,
            phone_https: null,
            snapshot: null,
          },
          excluded: ["phone-identifiers", "fingerprint", "local-paths", "contacts", "credentials"],
        };
        break;
      case "tls_clock":
        value = {
          revision: 1,
          value: {
            transferred: {},
            components: {},
            phone_clock: r.date,
            phone_https: null,
            snapshot: null,
          },
        };
        break;
      case "tls_progress":
        value = {
          revision: 0,
          value: {
            transferred: {},
            components: {},
            phone_clock: null,
            phone_https: null,
            snapshot: null,
          },
        };
        break;
      default:
        throw Error("Native Rust workbench is required for this operation (UI MOCK)");
    }
    return value as Calls[K]["out"];
  }
  async preparation(request: Parameters<WorkbenchBackend["preparation"]>[0]): Promise<PrepView> {
    if (request.kind === "record")
      this.steps.steps[request.step] = {
        status: request.status,
        source: "owner-manual-report; UI MOCK",
        checked_at_ms: Date.now(),
      };
    return {
      key: "mock-preparation",
      context: {
        fingerprint: "offline",
        transport: "mock",
        driver_signature: "mock",
        mode: "standard",
        profile: this.profile,
      },
      progress: { revision: 0, value: structuredClone(this.steps) },
    };
  }
  async exportPreparation(_deviceId: string | null): Promise<Readiness> {
    void _deviceId;
    return {
      schema: 1,
      bench: "E72-1",
      ...this.profile,
      transport: "mock",
      tls_manifest: "UI MOCK",
      steps: structuredClone(this.steps.steps),
      ready: false,
      excluded: ["phone-identifiers", "contacts", "APN", "passwords", "local-paths"],
    };
  }
  async phoneRead(_deviceId: string): Promise<PhoneSnapshot> {
    void _deviceId;
    throw Error("No physical PIM provider in UI MOCK");
  }
  async stageTls(_deviceId: string, _id: string, _target: Target): Promise<string> {
    void _deviceId;
    void _id;
    void _target;
    throw Error("Native file provider required (UI MOCK)");
  }
  async openAuthor(_id: string) {
    void _id;
    throw Error("Native browser action unavailable in UI MOCK");
  }
}
