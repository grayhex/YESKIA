const paths = {
  refresh: "M20 7v5h-5M4 17v-5h5M5 9a8 8 0 0 1 14-2l1 1M19 15a8 8 0 0 1-14 2l-1-1",
  download: "M12 3v12m-4-4 4 4 4-4M4 17v3h16v-3",
  upload: "M12 16V4m-4 4 4-4 4 4M4 17v3h16v-3",
  file: "M5 3h9l5 5v13H5ZM14 3v6h5",
  folder: "M3 7h7l2 2h9v11H3ZM3 7V4h7l2 3",
  plus: "M12 4v16M4 12h16",
  shield: "M12 2 3 6v6c0 6 9 10 9 10s9-4 9-10V6ZM8 12l3 3 5-6",
};
export function Icon({ name }: { name: keyof typeof paths }) {
  return (
    <svg
      className="button-icon"
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
