import i18n from "i18next";
import { initReactI18next } from "react-i18next";

const LOCALE_STORAGE_KEY = "aletheia.locale";
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
      delete: "Delete"
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
      delete: "Hichapụ"
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
      delete: "Parẹ́"
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
      delete: "Share"
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
