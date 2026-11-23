import en from '../locales/en.json';
import vi from '../locales/vi.json';

export type Lang = 'vi' | 'en';
const tables: Record<Lang, Record<string, string>> = { en, vi };

let current = $state<Lang>('en');

export function setLang(l: Lang): void {
  current = l;
}

export function getLang(): Lang {
  return current;
}

/** Looks a key up in the current language, then in English, then returns the key itself. */
export function translate(lang: Lang, key: string): string {
  return tables[lang][key] ?? tables.en[key] ?? key;
}

export function t(key: string): string {
  return translate(current, key);
}
