import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

// Standard Shadcn cn() helper.
export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}

/** Strip dashes from a UUID string: `7ab193b5-9cf3-...` → `7ab193b59cf3...` */
export function compactUuid(uuid: string): string {
  return uuid.replaceAll("-", "");
}
