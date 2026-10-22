import {getIdentifier,getName,getVersion} from '@tauri-apps/api/app';
import {openExternalUrl} from '$lib/services/platform';
import {copyTextToClipboard} from '$lib/services/clipboard';
import type {AboutGateway} from '$lib/features/mobile/about';
/** Actual host metadata and platform commands; no validation-crate version. */
export const mobileAboutGateway:AboutGateway={name:getName,version:getVersion,identifier:getIdentifier,open:openExternalUrl,copy:copyTextToClipboard};
