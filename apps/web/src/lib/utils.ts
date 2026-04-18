import { type ClassValue, clsx } from 'clsx'
import { twMerge } from 'tailwind-merge'

/**
 * Compose Tailwind class strings while resolving duplicates / conflicts.
 * The canonical helper exported by every shadcn-vue component — keep the
 * signature stable.
 */
export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs))
}
