/// USB language id.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Language {
    /// Afrikaans
    Afrikaans,
    /// Albanian
    Albanian,
    /// Arabic (Saudi Arabia)
    ArabicSaudiArabia,
    /// Arabic (Iraq)
    ArabicIraq,
    /// Arabic (Egypt)
    ArabicEgypt,
    /// Arabic (Libya)
    ArabicLibya,
    /// Arabic (Algeria)
    ArabicAlgeria,
    /// Arabic (Morocco)
    ArabicMorocco,
    /// Arabic (Tunisia)
    ArabicTunisia,
    /// Arabic (Oman)
    ArabicOman,
    /// Arabic (Yemen)
    ArabicYemen,
    /// Arabic (Syria)
    ArabicSyria,
    /// Arabic (Jordan)
    ArabicJordan,
    /// Arabic (Lebanon)
    ArabicLebanon,
    /// Arabic (Kuwait)
    ArabicKuwait,
    /// Arabic (UAE)
    ArabicUAE,
    /// Arabic (Bahrain)
    ArabicBahrain,
    /// Arabic (Qatar)
    ArabicQatar,
    /// Armenian
    Armenian,
    /// Assamese
    Assamese,
    /// Azeri (Latin)
    AzeriLatin,
    /// Azeri (Cyrillic)
    AzeriCyrillic,
    /// Basque
    Basque,
    /// Belarussian
    Belarussian,
    /// Bengali
    Bengali,
    /// Bulgarian
    Bulgarian,
    /// Burmese
    Burmese,
    /// Catalan
    Catalan,
    /// Chinese (Taiwan)
    ChineseTaiwan,
    /// Chinese (PRC)
    ChinesePRC,
    /// Chinese (Hong Kong SAR PRC)
    ChineseHongKongSARPRC,
    /// Chinese (Singapore)
    ChineseSingapore,
    /// Chinese (Macau SAR)
    ChineseMacauSAR,
    /// Croatian
    Croatian,
    /// Czech
    Czech,
    /// Danish
    Danish,
    /// Dutch (Netherlands)
    DutchNetherlands,
    /// Dutch (Belgium)
    DutchBelgium,
    /// English (United States)
    #[default]
    EnglishUnitedStates,
    /// English (United Kingdom)
    EnglishUnitedKingdom,
    /// English (Australian)
    EnglishAustralian,
    /// English (Canadian)
    EnglishCanadian,
    /// English (New Zealand)
    EnglishNewZealand,
    /// English (Ireland)
    EnglishIreland,
    /// English (South Africa)
    EnglishSouthAfrica,
    /// English (Jamaica)
    EnglishJamaica,
    /// English (Caribbean)
    EnglishCaribbean,
    /// English (Belize)
    EnglishBelize,
    /// English (Trinidad)
    EnglishTrinidad,
    /// English (Zimbabwe)
    EnglishZimbabwe,
    /// English (Philippines)
    EnglishPhilippines,
    /// Estonian
    Estonian,
    /// Faeroese
    Faeroese,
    /// Farsi
    Farsi,
    /// Finnish
    Finnish,
    /// French (Standard)
    FrenchStandard,
    /// French (Belgian)
    FrenchBelgian,
    /// French (Canadian)
    FrenchCanadian,
    /// French (Switzerland)
    FrenchSwitzerland,
    /// French (Luxembourg)
    FrenchLuxembourg,
    /// French (Monaco)
    FrenchMonaco,
    /// Georgian
    Georgian,
    /// German (Standard)
    GermanStandard,
    /// German (Switzerland)
    GermanSwitzerland,
    /// German (Austria)
    GermanAustria,
    /// German (Luxembourg)
    GermanLuxembourg,
    /// German (Liechtenstein)
    GermanLiechtenstein,
    /// Greek
    Greek,
    /// Gujarati
    Gujarati,
    /// Hebrew
    Hebrew,
    /// Hindi
    Hindi,
    /// Hungarian
    Hungarian,
    /// Icelandic
    Icelandic,
    /// Indonesian
    Indonesian,
    /// Italian (Standard)
    ItalianStandard,
    /// Italian (Switzerland)
    ItalianSwitzerland,
    /// Japanese
    Japanese,
    /// Kannada
    Kannada,
    /// KashmiriIndia
    KashmiriIndia,
    /// Kazakh
    Kazakh,
    /// Konkani
    Konkani,
    /// Korean
    Korean,
    /// Korean (Johab)
    KoreanJohab,
    /// Latvian
    Latvian,
    /// Lithuanian
    Lithuanian,
    /// Lithuanian (Classic)
    LithuanianClassic,
    /// Macedonian
    Macedonian,
    /// Malay (Malaysian)
    MalayMalaysian,
    /// Malay (Brunei Darussalam)
    MalayBruneiDarussalam,
    /// Malayalam
    Malayalam,
    /// Manipuri
    Manipuri,
    /// Marathi
    Marathi,
    /// Nepali (India)
    NepaliIndia,
    /// Norwegian (Bokmal)
    NorwegianBokmal,
    /// Norwegian (Nynorsk)
    NorwegianNynorsk,
    /// Oriya
    Oriya,
    /// Polish
    Polish,
    /// Portuguese (Brazil)
    PortugueseBrazil,
    /// Portuguese (Standard)
    PortugueseStandard,
    /// Punjabi
    Punjabi,
    /// Romanian
    Romanian,
    /// Russian
    Russian,
    /// Sanskrit
    Sanskrit,
    /// Serbian (Cyrillic)
    SerbianCyrillic,
    /// Serbian (Latin)
    SerbianLatin,
    /// Sindhi
    Sindhi,
    /// Slovak
    Slovak,
    /// Slovenian
    Slovenian,
    /// Spanish (Traditional Sort)
    SpanishTraditionalSort,
    /// Spanish (Mexican)
    SpanishMexican,
    /// Spanish (ModernSort)
    SpanishModernSort,
    /// Spanish (Guatemala)
    SpanishGuatemala,
    /// Spanish (Costa Rica)
    SpanishCostaRica,
    /// Spanish (Panama)
    SpanishPanama,
    /// Spanish (Dominican Republic)
    SpanishDominicanRepublic,
    /// Spanish (Venezuela)
    SpanishVenezuela,
    /// Spanish (Colombia)
    SpanishColombia,
    /// Spanish (Peru)
    SpanishPeru,
    /// Spanish (Argentina)
    SpanishArgentina,
    /// Spanish (Ecuador)
    SpanishEcuador,
    /// Spanish (Chile)
    SpanishChile,
    /// Spanish (Uruguay)
    SpanishUruguay,
    /// Spanish (Paraguay)
    SpanishParaguay,
    /// Spanish (Bolivia)
    SpanishBolivia,
    /// Spanish (El Salvador)
    SpanishElSalvador,
    /// Spanish (Honduras)
    SpanishHonduras,
    /// Spanish (Nicaragua)
    SpanishNicaragua,
    /// Spanish (Puerto Rico)
    SpanishPuertoRico,
    /// Sutu
    Sutu,
    /// Swahili (Kenya)
    SwahiliKenya,
    /// Swedish
    Swedish,
    /// Swedish (Finland)
    SwedishFinland,
    /// Tamil
    Tamil,
    /// Tatar (Tatarstan)
    TatarTatarstan,
    /// Telugu
    Telugu,
    /// Thai
    Thai,
    /// Turkish
    Turkish,
    /// Ukrainian
    Ukrainian,
    /// Urdu (Pakistan)
    UrduPakistan,
    /// Urdu (India)
    UrduIndia,
    /// Uzbek (Latin)
    UzbekLatin,
    /// Uzbek (Cyrillic)
    UzbekCyrillic,
    /// Vietnamese
    Vietnamese,
    /// HID usage data descriptor
    HidUsageDataDescriptor,
    /// HID vendor defined 1
    HidVendorDefined1,
    /// HID vendor defined 2
    HidVendorDefined2,
    /// HID vendor defined 3
    HidVendorDefined3,
    /// HID vendor defined 4
    HidVendorDefined4,
    /// Custom language code
    Other(u16),
}

impl From<Language> for u16 {
    fn from(lang: Language) -> u16 {
        match lang {
            Language::Afrikaans => 0x0436,
            Language::Albanian => 0x041c,
            Language::ArabicSaudiArabia => 0x0401,
            Language::ArabicIraq => 0x0801,
            Language::ArabicEgypt => 0x0c01,
            Language::ArabicLibya => 0x1001,
            Language::ArabicAlgeria => 0x1401,
            Language::ArabicMorocco => 0x1801,
            Language::ArabicTunisia => 0x1c01,
            Language::ArabicOman => 0x2001,
            Language::ArabicYemen => 0x2401,
            Language::ArabicSyria => 0x2801,
            Language::ArabicJordan => 0x2c01,
            Language::ArabicLebanon => 0x3001,
            Language::ArabicKuwait => 0x3401,
            Language::ArabicUAE => 0x3801,
            Language::ArabicBahrain => 0x3c01,
            Language::ArabicQatar => 0x4001,
            Language::Armenian => 0x042b,
            Language::Assamese => 0x044d,
            Language::AzeriLatin => 0x042c,
            Language::AzeriCyrillic => 0x082c,
            Language::Basque => 0x042d,
            Language::Belarussian => 0x0423,
            Language::Bengali => 0x0445,
            Language::Bulgarian => 0x0402,
            Language::Burmese => 0x0455,
            Language::Catalan => 0x0403,
            Language::ChineseTaiwan => 0x0404,
            Language::ChinesePRC => 0x0804,
            Language::ChineseHongKongSARPRC => 0x0c04,
            Language::ChineseSingapore => 0x1004,
            Language::ChineseMacauSAR => 0x1404,
            Language::Croatian => 0x041a,
            Language::Czech => 0x0405,
            Language::Danish => 0x0406,
            Language::DutchNetherlands => 0x0413,
            Language::DutchBelgium => 0x0813,
            Language::EnglishUnitedStates => 0x0409,
            Language::EnglishUnitedKingdom => 0x0809,
            Language::EnglishAustralian => 0x0c09,
            Language::EnglishCanadian => 0x1009,
            Language::EnglishNewZealand => 0x1409,
            Language::EnglishIreland => 0x1809,
            Language::EnglishSouthAfrica => 0x1c09,
            Language::EnglishJamaica => 0x2009,
            Language::EnglishCaribbean => 0x2409,
            Language::EnglishBelize => 0x2809,
            Language::EnglishTrinidad => 0x2c09,
            Language::EnglishZimbabwe => 0x3009,
            Language::EnglishPhilippines => 0x3409,
            Language::Estonian => 0x0425,
            Language::Faeroese => 0x0438,
            Language::Farsi => 0x0429,
            Language::Finnish => 0x040b,
            Language::FrenchStandard => 0x040c,
            Language::FrenchBelgian => 0x080c,
            Language::FrenchCanadian => 0x0c0c,
            Language::FrenchSwitzerland => 0x100c,
            Language::FrenchLuxembourg => 0x140c,
            Language::FrenchMonaco => 0x180c,
            Language::Georgian => 0x0437,
            Language::GermanStandard => 0x0407,
            Language::GermanSwitzerland => 0x0807,
            Language::GermanAustria => 0x0c07,
            Language::GermanLuxembourg => 0x1007,
            Language::GermanLiechtenstein => 0x1407,
            Language::Greek => 0x0408,
            Language::Gujarati => 0x0447,
            Language::Hebrew => 0x040d,
            Language::Hindi => 0x0439,
            Language::Hungarian => 0x040e,
            Language::Icelandic => 0x040f,
            Language::Indonesian => 0x0421,
            Language::ItalianStandard => 0x0410,
            Language::ItalianSwitzerland => 0x0810,
            Language::Japanese => 0x0411,
            Language::Kannada => 0x044b,
            Language::KashmiriIndia => 0x0860,
            Language::Kazakh => 0x043f,
            Language::Konkani => 0x0457,
            Language::Korean => 0x0412,
            Language::KoreanJohab => 0x0812,
            Language::Latvian => 0x0426,
            Language::Lithuanian => 0x0427,
            Language::LithuanianClassic => 0x0827,
            Language::Macedonian => 0x042f,
            Language::MalayMalaysian => 0x043e,
            Language::MalayBruneiDarussalam => 0x083e,
            Language::Malayalam => 0x044c,
            Language::Manipuri => 0x0458,
            Language::Marathi => 0x044e,
            Language::NepaliIndia => 0x0861,
            Language::NorwegianBokmal => 0x0414,
            Language::NorwegianNynorsk => 0x0814,
            Language::Oriya => 0x0448,
            Language::Polish => 0x0415,
            Language::PortugueseBrazil => 0x0416,
            Language::PortugueseStandard => 0x0816,
            Language::Punjabi => 0x0446,
            Language::Romanian => 0x0418,
            Language::Russian => 0x0419,
            Language::Sanskrit => 0x044f,
            Language::SerbianCyrillic => 0x0c1a,
            Language::SerbianLatin => 0x081a,
            Language::Sindhi => 0x0459,
            Language::Slovak => 0x041b,
            Language::Slovenian => 0x0424,
            Language::SpanishTraditionalSort => 0x040a,
            Language::SpanishMexican => 0x080a,
            Language::SpanishModernSort => 0x0c0a,
            Language::SpanishGuatemala => 0x100a,
            Language::SpanishCostaRica => 0x140a,
            Language::SpanishPanama => 0x180a,
            Language::SpanishDominicanRepublic => 0x1c0a,
            Language::SpanishVenezuela => 0x200a,
            Language::SpanishColombia => 0x240a,
            Language::SpanishPeru => 0x280a,
            Language::SpanishArgentina => 0x2c0a,
            Language::SpanishEcuador => 0x300a,
            Language::SpanishChile => 0x340a,
            Language::SpanishUruguay => 0x380a,
            Language::SpanishParaguay => 0x3c0a,
            Language::SpanishBolivia => 0x400a,
            Language::SpanishElSalvador => 0x440a,
            Language::SpanishHonduras => 0x480a,
            Language::SpanishNicaragua => 0x4c0a,
            Language::SpanishPuertoRico => 0x500a,
            Language::Sutu => 0x0430,
            Language::SwahiliKenya => 0x0441,
            Language::Swedish => 0x041d,
            Language::SwedishFinland => 0x081d,
            Language::Tamil => 0x0449,
            Language::TatarTatarstan => 0x0444,
            Language::Telugu => 0x044a,
            Language::Thai => 0x041e,
            Language::Turkish => 0x041f,
            Language::Ukrainian => 0x0422,
            Language::UrduPakistan => 0x0420,
            Language::UrduIndia => 0x0820,
            Language::UzbekLatin => 0x0443,
            Language::UzbekCyrillic => 0x0843,
            Language::Vietnamese => 0x042a,
            Language::HidUsageDataDescriptor => 0x04ff,
            Language::HidVendorDefined1 => 0xf0ff,
            Language::HidVendorDefined2 => 0xf4ff,
            Language::HidVendorDefined3 => 0xf8ff,
            Language::HidVendorDefined4 => 0xfcff,
            Language::Other(other) => other,
        }
    }
}
