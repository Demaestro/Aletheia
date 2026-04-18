import i18n from "i18next";
import { initReactI18next } from "react-i18next";

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
    }
  }
};

i18n
  .use(initReactI18next)
  .init({
    resources,
    lng: "en",
    fallbackLng: "en",
    interpolation: {
      escapeValue: false
    }
  });

export default i18n;
