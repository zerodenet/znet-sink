import { flagCodeFromEmoji } from '$lib/services/node-utils';

/** Keep raw policy tags intact; replace flags only in their presentation. */
export function flagTextParts(text: string): { text: string; countryCode?: string }[] {
  return text.split(/(\p{Regional_Indicator}{2})/u).filter(Boolean).map((part) => {
    const code = flagCodeFromEmoji(part);
    return code ? { text: part, countryCode: code.toLowerCase() } : { text: part };
  });
}
