import { cn } from "@/lib/utils";

/**
 * The Helix mark: four entities joined in a loop. Filled with the signal colour
 * so the brand and the one primary action in each view read as the same voice.
 */
export function BrandMark({ size = 20, className }: { size?: number; className?: string }) {
  return (
    <svg
      className={cn("brand-mark", className)}
      width={size}
      height={size}
      viewBox="0 0 64 64"
      aria-hidden="true"
      focusable="false"
    >
      <rect x="4" y="4" width="56" height="56" rx="15" className="brand-mark-tile" />
      <path d="M19 39 31 22l14 9-12 14Z" className="brand-mark-edge" />
      <circle cx="19" cy="39" r="4.5" className="brand-mark-node" />
      <circle cx="31" cy="22" r="4.5" className="brand-mark-node" />
      <circle cx="45" cy="31" r="4.5" className="brand-mark-node" />
      <circle cx="33" cy="45" r="4.5" className="brand-mark-node" />
    </svg>
  );
}
