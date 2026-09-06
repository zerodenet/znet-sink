const COUNTRY_CODES: Record<string, string> = {
    '中国': 'CN', '美国': 'US', '日本': 'JP', '韩国': 'KR', '新加坡': 'SG',
    '香港': 'HK', '台湾': 'TW', '澳门': 'MO', '英国': 'GB', '德国': 'DE',
    '法国': 'FR', '加拿大': 'CA', '澳大利亚': 'AU', '俄罗斯': 'RU', '印度': 'IN',
    '巴西': 'BR', '荷兰': 'NL', '瑞典': 'SE', '瑞士': 'CH', '芬兰': 'FI',
    '挪威': 'NO', '丹麦': 'DK', '波兰': 'PL', '捷克': 'CZ', '奥地利': 'AT',
    '比利时': 'BE', '意大利': 'IT', '西班牙': 'ES', '葡萄牙': 'PT', '爱尔兰': 'IE',
    '新西兰': 'NZ', '墨西哥': 'MX', '阿根廷': 'AR', '智利': 'CL', '南非': 'ZA',
    '泰国': 'TH', '越南': 'VN', '马来西亚': 'MY', '印度尼西亚': 'ID', '菲律宾': 'PH',
    '阿联酋': 'AE', '沙特阿拉伯': 'SA', '以色列': 'IL', '土耳其': 'TR', '乌克兰': 'UA',
    'china': 'CN', 'united states': 'US', 'usa': 'US', 'japan': 'JP',
    'south korea': 'KR', 'korea': 'KR', 'singapore': 'SG', 'hong kong': 'HK',
    'taiwan': 'TW', 'united kingdom': 'GB', 'uk': 'GB', 'germany': 'DE',
    'france': 'FR', 'canada': 'CA', 'australia': 'AU', 'russia': 'RU',
    'india': 'IN', 'brazil': 'BR', 'netherlands': 'NL', 'sweden': 'SE',
    'switzerland': 'CH', 'finland': 'FI', 'norway': 'NO', 'denmark': 'DK',
    'poland': 'PL', 'czech republic': 'CZ', 'czechia': 'CZ', 'austria': 'AT',
    'belgium': 'BE', 'italy': 'IT', 'spain': 'ES', 'portugal': 'PT',
    'ireland': 'IE', 'new zealand': 'NZ', 'mexico': 'MX', 'argentina': 'AR',
    'chile': 'CL', 'south africa': 'ZA', 'thailand': 'TH', 'vietnam': 'VN',
    'malaysia': 'MY', 'indonesia': 'ID', 'philippines': 'PH',
    'united arab emirates': 'AE', 'saudi arabia': 'SA', 'israel': 'IL',
    'turkey': 'TR', 'ukraine': 'UA', 'macao': 'MO', 'macau': 'MO',
  };

const regionNames = new Intl.DisplayNames(['zh-CN'], { type: 'region' });

export function networkLocation(probe?: { country?: string; region?: string; city?: string } | null) {
  const country = probe?.country?.trim() ?? '';
  const code = /^[a-z]{2}$/i.test(country) ? country.toUpperCase() : COUNTRY_CODES[country.toLowerCase()] ?? COUNTRY_CODES[country];
  const localized = code ? regionNames.of(code) : undefined;
  const countryCode = code && code !== 'ZZ' && localized !== code ? code.toLowerCase() : undefined;
  const location = [...new Set([countryCode ? localized : country, probe?.region, probe?.city].map(value => value?.trim()).filter(Boolean))].join(' · ');
  return { countryCode, location: location || '地区未知' };
}
