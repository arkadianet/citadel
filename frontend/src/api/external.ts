/**
 * Open external URLs in the system browser.
 *
 * Uses the Tauri opener plugin so links work from within the webview.
 */

import { openUrl } from '@tauri-apps/plugin-opener'

/** Open a URL in the user's default browser. */
export function openExternal(url: string) {
  openUrl(url).catch(err => console.error('Failed to open URL:', err))
}
