// Maps national football team names (and common TxLINE variants) to lowercase
// flag-icons codes. Standard countries use ISO 3166-1 alpha-2 lowercased.
// GB home nations use flag-icons sub-codes: gb-eng, gb-sct, gb-wls, gb-nir.
// Returns undefined when no match is found so the caller can hide the element.

const TEAM_TO_CODE: Record<string, string> = {
  // A
  afghanistan: 'af',
  albania: 'al',
  algeria: 'dz',
  angola: 'ao',
  argentina: 'ar',
  armenia: 'am',
  australia: 'au',
  austria: 'at',
  azerbaijan: 'az',
  // B
  bahrain: 'bh',
  bangladesh: 'bd',
  belgium: 'be',
  bolivia: 'bo',
  bosnia: 'ba',
  'bosnia and herzegovina': 'ba',
  brazil: 'br',
  brasil: 'br',
  bulgaria: 'bg',
  'burkina faso': 'bf',
  // C
  cameroon: 'cm',
  canada: 'ca',
  chile: 'cl',
  china: 'cn',
  'china pr': 'cn',
  colombia: 'co',
  congo: 'cg',
  'costa rica': 'cr',
  croatia: 'hr',
  cuba: 'cu',
  cyprus: 'cy',
  'czech republic': 'cz',
  czechia: 'cz',
  // D
  denmark: 'dk',
  // E
  ecuador: 'ec',
  egypt: 'eg',
  'el salvador': 'sv',
  england: 'gb-eng',
  eritrea: 'er',
  estonia: 'ee',
  ethiopia: 'et',
  // F
  finland: 'fi',
  france: 'fr',
  // G
  gabon: 'ga',
  gambia: 'gm',
  georgia: 'ge',
  germany: 'de',
  ghana: 'gh',
  gibraltar: 'gi',
  greece: 'gr',
  guatemala: 'gt',
  guinea: 'gn',
  // H
  haiti: 'ht',
  honduras: 'hn',
  hungary: 'hu',
  // I
  iceland: 'is',
  india: 'in',
  indonesia: 'id',
  iran: 'ir',
  iraq: 'iq',
  ireland: 'ie',
  'republic of ireland': 'ie',
  israel: 'il',
  italy: 'it',
  'ivory coast': 'ci',
  "côte d'ivoire": 'ci',
  'cote divoire': 'ci',
  // J
  jamaica: 'jm',
  japan: 'jp',
  jordan: 'jo',
  // K
  kazakhstan: 'kz',
  kenya: 'ke',
  kuwait: 'kw',
  kyrgyzstan: 'kg',
  // L
  latvia: 'lv',
  lebanon: 'lb',
  liberia: 'lr',
  libya: 'ly',
  lithuania: 'lt',
  luxembourg: 'lu',
  // M
  malaysia: 'my',
  mali: 'ml',
  malta: 'mt',
  mexico: 'mx',
  moldova: 'md',
  montenegro: 'me',
  morocco: 'ma',
  mozambique: 'mz',
  // N
  namibia: 'na',
  nepal: 'np',
  netherlands: 'nl',
  holland: 'nl',
  'new zealand': 'nz',
  nicaragua: 'ni',
  nigeria: 'ng',
  'north korea': 'kp',
  'north macedonia': 'mk',
  'northern ireland': 'gb-nir',
  norway: 'no',
  // O
  oman: 'om',
  // P
  pakistan: 'pk',
  panama: 'pa',
  paraguay: 'py',
  peru: 'pe',
  philippines: 'ph',
  poland: 'pl',
  portugal: 'pt',
  // Q
  qatar: 'qa',
  // R
  romania: 'ro',
  russia: 'ru',
  // S
  'saudi arabia': 'sa',
  scotland: 'gb-sct',
  senegal: 'sn',
  serbia: 'rs',
  'sierra leone': 'sl',
  slovakia: 'sk',
  slovenia: 'si',
  somalia: 'so',
  'south africa': 'za',
  'south korea': 'kr',
  'korea republic': 'kr',
  'korea rep': 'kr',
  spain: 'es',
  'sri lanka': 'lk',
  sweden: 'se',
  switzerland: 'ch',
  syria: 'sy',
  // T
  taiwan: 'tw',
  tajikistan: 'tj',
  tanzania: 'tz',
  thailand: 'th',
  togo: 'tg',
  'trinidad and tobago': 'tt',
  tunisia: 'tn',
  turkey: 'tr',
  türkiye: 'tr',
  turkmenistan: 'tm',
  // U
  uganda: 'ug',
  ukraine: 'ua',
  'united arab emirates': 'ae',
  uae: 'ae',
  'united states': 'us',
  usa: 'us',
  us: 'us',
  uruguay: 'uy',
  uzbekistan: 'uz',
  // V
  venezuela: 've',
  vietnam: 'vn',
  // W
  wales: 'gb-wls',
  // Y
  yemen: 'ye',
  // Z
  zambia: 'zm',
  zimbabwe: 'zw',
}

/**
 * Returns the flag-icons CSS code for a national team name, or undefined if
 * not found. Use as: `<span className={`fi fi-${teamIso(name)}`} />`
 * Matching is case-insensitive and trims whitespace.
 */
export function teamIso(name: string): string | undefined {
  if (!name) return undefined
  return TEAM_TO_CODE[name.trim().toLowerCase()]
}
