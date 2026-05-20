import i18n from "i18next";
import { initReactI18next } from "react-i18next";

const LOCALE_STORAGE_KEY = "aletheia:locale";
const SUPPORTED = ["en", "ig", "yo", "ha"] as const;
type Locale = (typeof SUPPORTED)[number];

const resources = {
  en: {
    navigation: {
      landing: { title: "Landing", kicker: "Public page" },
      dashboard: { title: "Operator dashboard", kicker: "Sunday AM service" },
      transcript: { title: "Live transcript", kicker: "Speech detection" },
      queue: { title: "Queue and approval", kicker: "Scripture candidates" },
      output: { title: "Preview and live output", kicker: "Presentation control" },
      theme: { title: "Theme designer", kicker: "Broadcast styling" },
      integrations: { title: "Integrations settings", kicker: "Vendor adapters" },
      health: { title: "Offline health", kicker: "Pre-service readiness" },
      onboarding: { title: "Onboarding flow", kicker: "Volunteer setup" },
      search: { title: "Manual search", kicker: "Fallback workflow" }
    },
    status: {
      coreOnline: "Core online",
      browserMode: "Browser mode"
    },
    common: {
      save: "Save",
      cancel: "Cancel",
      preview: "Preview",
      live: "Take live",
      clear: "Clear",
      check: "Check",
      import: "Import",
      export: "Export",
      delete: "Delete",
      runCheck: "Run pre-service check",
      runRehearsal: "Run local rehearsal",
      runAiAssist: "Run AI assist",
      approvePreview: "Approve to preview"
    },
    dashboard: {
      eyebrow: "Operator dashboard",
      title: "Current service state",
      detail: "The dashboard keeps the next safe action visible: listen, verify, preview, then send live.",
      sourceMap: "Source map",
      aiAssist: "AI scripture assist",
      aiDecision: "AI decision",
      multilingualRouting: "Multilingual routing",
      detectedLanguages: "Detected languages",
      accuracyTarget: "Accuracy target",
      latestTranscript: "Latest transcript",
      destinationReadiness: "Destination readiness"
    },
    health: {
      eyebrow: "Offline and health",
      title: "Know what still works before the service starts",
      detail: "The health panel prioritizes blocking issues first and translates technical failures into operator actions.",
      offlineReadiness: "Offline readiness",
      acceptanceTesting: "Device acceptance testing",
      offlinePack: "Offline distribution pack",
      offlineAssets: "Offline assets",
      productionReadiness: "Production readiness",
      releaseGates: "Release gates",
      rehearsalRunner: "Local rehearsal runner",
      securityBlockers: "Security blockers",
      supportBundle: "Support bundle",
      deviceSummary: "Device summary",
      lowBandwidth: "Low-bandwidth policy"
    },
    integrations: {
      eyebrow: "Output adapters",
      title: "Where scripture goes when you press Live",
      vmixBridge: "vMix bridge",
      operatorName: "Operator identity",
      adapterPolicy: "Adapter policy"
    }
  },
  ig: {
    navigation: {
      landing: { title: "Ihu Mmalite", kicker: "Ihu Ọha" },
      dashboard: { title: "Ngwaọrụ Onye Ọrụ", kicker: "Ozi Ụtụtụ Sọnde" },
      transcript: { title: "Ndepụta Okwu Dị Ndụ", kicker: "Nchọpụta Olu" },
      queue: { title: "Ahịrị & Nkwado", kicker: "Akwụkwọ Nsọ A Họọrọ" },
      output: { title: "Nnwale & Ngosipụta Dị Ndụ", kicker: "Njikwa Ngosipụta" },
      theme: { title: "Onye Nwe Nhazi", kicker: "Nhazi Mgbasa Ozi" },
      integrations: { title: "Ntọala Njikọ", kicker: "Ngwa Nkwụnye" },
      health: { title: "Ahụike Ntanetị", kicker: "Nkwadebe Tupu Ozi" },
      onboarding: { title: "Usoro Mmalite", kicker: "Nhazi Ndị Ọrụ Afọ Ofufo" },
      search: { title: "Nchọgharị Aka", kicker: "Usoro Nchọgharị" }
    },
    status: {
      coreOnline: "Njikwa Ntanetị",
      browserMode: "Ụdị Ihe Nchọgharị"
    },
    common: {
      save: "Chekwaa",
      cancel: "Kagbuo",
      preview: "Nlele",
      live: "Bido Ngosi",
      clear: "Hichapụ",
      check: "Lelee",
      import: "Bubata",
      export: "Bupụ",
      delete: "Hichapụ",
      runCheck: "Mee Nlele Tupu Ozi",
      runRehearsal: "Gaa Próva Ebe A",
      runAiAssist: "Mee Nkwado AI",
      approvePreview: "Kwado maka Nlele"
    },
    dashboard: {
      eyebrow: "Ngwaọrụ Onye Ọrụ",
      title: "Ọnọdụ Ozi Ugbu a",
      detail: "Ngwaọrụ na-egosi ọrụ na-abata ọzọ: nụ, lelee, nlele, wee zipu ndụ.",
      sourceMap: "Ihe Isi Ozuzo",
      aiAssist: "Nkwado AI Akwụkwọ Nsọ",
      aiDecision: "Mkpebi AI",
      multilingualRouting: "Ụzọ Asụsụ Dị Iche Iche",
      detectedLanguages: "Asụsụ A Chọpụtara",
      accuracyTarget: "Ebumnuche Izi Ezi",
      latestTranscript: "Ndepụta Ikpeazụ",
      destinationReadiness: "Nkwadebe Ebe Mgbasa"
    },
    health: {
      eyebrow: "Ahụike Ntanetị",
      title: "Mara ihe na-arụ ọrụ tupu ozi amalite",
      detail: "Ihe ngosi ahụike na-etinye nsogbu ndọgbu ụzọ mbụ.",
      offlineReadiness: "Nkwadebe Ntanetị",
      acceptanceTesting: "Nnwale Ụlọ Ọrụ",
      offlinePack: "Ngwugwu Ọrụ n'oge Ntanetị",
      offlineAssets: "Akụ Ntanetị",
      productionReadiness: "Nkwadebe Mmepụta",
      releaseGates: "Ọnụ Ụzọ Ntọhapụ",
      rehearsalRunner: "Próva Ebe A",
      securityBlockers: "Mgbochi Nchedo",
      supportBundle: "Ngwugwu Nkwado",
      deviceSummary: "Nchịkọta Ngwaọrụ",
      lowBandwidth: "Iwu Ọnụ Ụzọ Ntakịrị"
    },
    integrations: {
      eyebrow: "Ngwa Mmepụta",
      title: "Ebe Akwụkwọ Nsọ na-aga mgbe ị naanya Ndụ",
      vmixBridge: "Ngịga vMix",
      operatorName: "Njirimara Onye Ọrụ",
      adapterPolicy: "Iwu Ngwa"
    }
  },
  yo: {
    navigation: {
      landing: { title: "Ojú-ìwé Ìbẹ̀rẹ̀", kicker: "Ojú-ìwé Gbangba" },
      dashboard: { title: "Pẹpẹ Olùṣakoso", kicker: "Ìsìn Àárọ̀ Sunday" },
      transcript: { title: "Àkọsílẹ̀ Ọ̀rọ̀ Ìṣẹ̀lẹ̀", kicker: "Ìdánimọ̀ Ohùn" },
      queue: { title: "Ìlà & Ìfọwọ́sí", kicker: "Àwọn Ọ̀rọ̀ Ìwé Mímọ́" },
      output: { title: "Àyẹ̀wò & Ìfihàn Tààrà", kicker: "Ìṣàkóso Ìgbékalẹ̀" },
      theme: { title: "Olùṣe Àpẹẹrẹ", kicker: "Àpẹẹrẹ Ìgbóhùnsáfẹ́fẹ́" },
      integrations: { title: "Ètò Ìṣọ̀kan", kicker: "Àwọn Olùpèsè" },
      health: { title: "Ìlera Aláìsí Íńtánẹ́tì", kicker: "Ìmúrasílẹ̀ Ìṣáájú-Ìsìn" },
      onboarding: { title: "Ìṣílẹ̀kùn Tuntun", kicker: "Ètò Olùyọ̀ǹda" },
      search: { title: "Ìwákiri Ọwọ́", kicker: "Ipa Yíyàn" }
    },
    status: {
      coreOnline: "Ìpìlẹ̀ Ń Ṣiṣẹ́",
      browserMode: "Ipo Aṣàwákiri"
    },
    common: {
      save: "Fi Pamọ́",
      cancel: "Fagilé",
      preview: "Àyẹ̀wò",
      live: "Bẹ̀rẹ̀ Tààrà",
      clear: "Pa Rẹ́",
      check: "Ṣàyẹ̀wò",
      import: "Mú Wọlé",
      export: "Gbé Jáde",
      delete: "Parẹ́",
      runCheck: "Ṣe Àyẹ̀wò Ìṣáájú-Ìsìn",
      runRehearsal: "Ṣe Àdánwò Ìbílẹ̀",
      runAiAssist: "Ṣe Ìrànlọ́wọ́ AI",
      approvePreview: "Fọwọ́sí fún Àyẹ̀wò"
    },
    dashboard: {
      eyebrow: "Pẹpẹ Olùṣakoso",
      title: "Ipo Ìsìn Lọ́wọ́lọ́wọ́",
      detail: "Pẹpẹ náà ń fi ìgbésẹ̀ àbò kó sí ìwò: gbọ́, ṣàyẹ̀wò, ìwo àyẹ̀wò, lẹ́hìnnà fi ránṣẹ́.",
      sourceMap: "Mápù Òrisun",
      aiAssist: "Ìrànlọ́wọ́ AI Ìwé Mímọ́",
      aiDecision: "Ìpinnu AI",
      multilingualRouting: "Ìtọ́sọ́nà Èdè Pọ̀",
      detectedLanguages: "Àwọn Èdè Tí A Mọ̀",
      accuracyTarget: "Àfojúsùn Ìpéye",
      latestTranscript: "Àkọsílẹ̀ Tó Gbẹ̀yìn",
      destinationReadiness: "Ìmúrasílẹ̀ Ìdojúkọ"
    },
    health: {
      eyebrow: "Ìlera Aláìsí Íńtánẹ́tì",
      title: "Mọ ohun tó ṣiṣẹ́ ṣáájú ìsìn",
      detail: "Àgbékalẹ̀ ìlera máa ń fi àwọn ìṣòro tó ń dínà sí iwájú.",
      offlineReadiness: "Ìmúrasílẹ̀ Aláìsí Íńtánẹ́tì",
      acceptanceTesting: "Ìdánwò Ìgbàwọlé Ẹ̀rọ",
      offlinePack: "Àpò Ìpínpín Aláìsí Íńtánẹ́tì",
      offlineAssets: "Àwọn Ohun-Ìní Aláìsí",
      productionReadiness: "Ìmúrasílẹ̀ Iṣẹ́",
      releaseGates: "Ẹ̀nubọ̀dé Ìtẹ̀síwájú",
      rehearsalRunner: "Àdánwò Ìbílẹ̀",
      securityBlockers: "Àwọn Àdínà Ààbò",
      supportBundle: "Àpò Ìrànlọ́wọ́",
      deviceSummary: "Àkọóròyìn Ẹ̀rọ",
      lowBandwidth: "Ìlànà Ìgbéká Kékeré"
    },
    integrations: {
      eyebrow: "Àwọn Adapta Àbájáde",
      title: "Ibo Ìwé Mímọ́ Ń lọ Nígbà Tí O Tẹ Tààrà",
      vmixBridge: "Afárá vMix",
      operatorName: "Ìdánimọ̀ Olùṣakoso",
      adapterPolicy: "Ìlànà Adapta"
    }
  },
  ha: {
    navigation: {
      landing: { title: "Shafin Farko", kicker: "Shafi na Jama'a" },
      dashboard: { title: "Allon Ma'aikaci", kicker: "Sabis na Asuba Lahadi" },
      transcript: { title: "Rubutu Kai Tsaye", kicker: "Gano Magana" },
      queue: { title: "Layi da Tabbatarwa", kicker: "Zaɓuɓɓukan Nassi" },
      output: { title: "Duba & Fitarwa Kai Tsaye", kicker: "Sarrafa Gabatarwa" },
      theme: { title: "Mai Tsara Salo", kicker: "Salon Watsa Shirye-shirye" },
      integrations: { title: "Saitin Haɗawa", kicker: "Adaftocin Mai Sayarwa" },
      health: { title: "Lafiyar Layi", kicker: "Shirin Pre-Sabis" },
      onboarding: { title: "Tafarkin Shigarwa", kicker: "Saitin Mai Sa Kai" },
      search: { title: "Bincike na Hannu", kicker: "Hanyar Madadin" }
    },
    status: {
      coreOnline: "Tsakiya Tana Aiki",
      browserMode: "Yanayin Mai Bincike"
    },
    common: {
      save: "Ajiye",
      cancel: "Soke",
      preview: "Duba",
      live: "Fara Kai Tsaye",
      clear: "Share",
      check: "Bincika",
      import: "Shigo da",
      export: "Fitar",
      delete: "Share",
      runCheck: "Gudanar da Binciken Pre-Sabis",
      runRehearsal: "Gudanar da Gwajin Gida",
      runAiAssist: "Gudanar da Taimakon AI",
      approvePreview: "Amince don Duba"
    },
    dashboard: {
      eyebrow: "Allon Ma'aikaci",
      title: "Yanayin Sabis na Yanzu",
      detail: "Allon yana nuna aikin da ya fi aminci: saurara, tabbatar, duba, sannan aika kai tsaye.",
      sourceMap: "Taswira Madogara",
      aiAssist: "Taimakon AI na Nassosi",
      aiDecision: "Shawarar AI",
      multilingualRouting: "Tsarin Shimfiɗa Harsuna",
      detectedLanguages: "Harsuna da aka Gano",
      accuracyTarget: "Manufar Daidaito",
      latestTranscript: "Rubuce-rubuce na Ƙarshe",
      destinationReadiness: "Shirye-shiryen Inda za a Aika"
    },
    health: {
      eyebrow: "Lafiyar Layi",
      title: "San abin da ke aiki kafin sabis ya fara",
      detail: "Hukumar Lafiya tana sanya matsalolin da ke toshe aiki a gaba.",
      offlineReadiness: "Shirye-shiryen Layi",
      acceptanceTesting: "Gwajin Karɓar Na'ura",
      offlinePack: "Fakitin Rarraba Layi",
      offlineAssets: "Kadarori na Layi",
      productionReadiness: "Shirye-shiryen Samarwa",
      releaseGates: "Ƙofofin Saki",
      rehearsalRunner: "Gwajin Gida",
      securityBlockers: "Masu Toshe Tsaro",
      supportBundle: "Fakitin Tallafi",
      deviceSummary: "Taƙaitaccen Ba'ana Na'ura",
      lowBandwidth: "Manufar Bandwith Ƙarami"
    },
    integrations: {
      eyebrow: "Adaftocin Fitarwa",
      title: "Inda Nassosi ke Tafiya Lokacin da ka Latsa Kai Tsaye",
      vmixBridge: "Gadiyar vMix",
      operatorName: "Shaida Ma'aikaci",
      adapterPolicy: "Manufar Adafta"
    }
  }
};

function loadInitialLocale(): Locale {
  if (typeof window === "undefined") return "en";
  const stored = window.localStorage.getItem(LOCALE_STORAGE_KEY);
  if (stored && (SUPPORTED as readonly string[]).includes(stored)) {
    return stored as Locale;
  }
  return "en";
}

i18n
  .use(initReactI18next)
  .init({
    resources,
    lng: loadInitialLocale(),
    fallbackLng: "en",
    interpolation: {
      escapeValue: false
    }
  });

if (typeof window !== "undefined") {
  i18n.on("languageChanged", (lng) => {
    if ((SUPPORTED as readonly string[]).includes(lng)) {
      window.localStorage.setItem(LOCALE_STORAGE_KEY, lng);
    }
  });
}

export default i18n;
