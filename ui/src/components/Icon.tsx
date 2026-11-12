const paths = {
  home: "M3 10 12 3l9 7v11h-6v-7H9v7H3Z",
  up: "M12 20V4m-6 6 6-6 6 6",
  phone: "M7 2h10v20H7ZM10 5h4M11 19h2",
  computer: "M3 4h18v13H3ZM8 21h8M12 17v4",
  bluetooth: "M7 7l10 10-5 5V2l5 5L7 17",
  usb: "M12 21V3m-3 3 3-3 3 3M12 15l-5-4V8m5 10 5-5V9M5 6h4v2H5ZM15 7h4v2h-4",
  contacts:
    "M16 21v-3a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v3M9 10a4 4 0 1 0 0-8 4 4 0 0 0 0 8M17 3a4 4 0 0 1 0 8M22 21v-3a4 4 0 0 0-3-4",
  message: "M3 4h18v13H8l-5 4ZM7 8h10M7 12h7",
  calendar: "M3 5h18v16H3ZM7 2v6M17 2v6M3 10h18M7 14h3M14 14h3",
  apps: "M3 3h7v7H3ZM14 3h7v7h-7ZM3 14h7v7H3ZM14 14h7v7h-7Z",
  music: "M9 18V5l11-2v13M9 7l11-2M9 18c0 3-6 4-6 1s6-4 6-1ZM20 16c0 3-6 4-6 1s6-4 6-1Z",
  image: "M3 3h18v18H3ZM3 17l6-6 4 4 3-3 5 5M8 7h.01",
  archive: "M3 4h18v5H3ZM5 9v12h14V9M9 13h6",
  activity: "M3 12h4l3-8 4 16 3-8h4",
  settings: "M4 7h16M4 17h16M9 4v6M15 14v6",
  info: "M12 11v6M12 7h.01M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0",
  alert: "M12 3 2 21h20ZM12 9v5M12 17h.01",
  chevron: "m9 5 7 7-7 7",

  refresh: "M20 7v5h-5M4 17v-5h5M5 9a8 8 0 0 1 14-2l1 1M19 15a8 8 0 0 1-14 2l-1-1",
  download: "M12 3v12m-4-4 4 4 4-4M4 17v3h16v-3",
  upload: "M12 16V4m-4 4 4-4 4 4M4 17v3h16v-3",
  file: "M5 3h9l5 5v13H5ZM14 3v6h5",
  folder: "M3 7h7l2 2h9v11H3ZM3 7V4h7l2 3",
  plus: "M12 4v16M4 12h16",
  shield: "M12 2 3 6v6c0 6 9 10 9 10s9-4 9-10V6ZM8 12l3 3 5-6",
};
export type IconName = keyof typeof paths;

export function Icon({ name, className = "button-icon" }: { name: IconName; className?: string }) {
  return (
    <svg
      className={className}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d={paths[name]} />
    </svg>
  );
}
