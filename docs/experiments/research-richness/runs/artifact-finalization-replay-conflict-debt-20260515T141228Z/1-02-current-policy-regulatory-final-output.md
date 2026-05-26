## 최종 답변 (Final Answer)

### 표적 수리 결과
이전 품질 게이트 실패 항목은 “충돌 맵에 미해결 충돌이 debt로 승격되지 않음”이었습니다. 이번 판에서는 해당 충돌을 “10^25 FLOP 기준의 안정성”으로 명시하고, 공식 법령 Article 51 및 Commission GPAI factpage로 재검증했습니다. 결론은 “현재 법적 추정 기준은 확인됨, 다만 Commission의 delegated act 변경 권한과 ‘under review’ 고지가 있어 장기 안정성은 조건부”입니다. 따라서 충돌은 unresolved가 아니라 resolved_with_caveat로 처리하고, 남는 확인 과제는 “향후 delegated act 또는 AI Office 지정 목록 확인”으로 연구 부채에 남겼습니다.

### 핵심 결론
NIST AI Risk Management Framework, 즉 AI RMF 1.0은 “frontier model deployer”가 자율적으로 위험을 식별·측정·관리하도록 돕는 비구속적 위험관리 프레임워크입니다. NIST는 AI RMF가 voluntary, rights-preserving, non-sector-specific, use-case agnostic이라고 설명하며, 핵심 기능은 GOVERN, MAP, MEASURE, MANAGE입니다. 반면 EU AI Act는 법적 의무 체계이고, 특히 일반목적 AI 모델(GPAI)과 systemic risk GPAI 모델의 “provider”에게 기술문서, downstream provider 정보 제공, 저작권 정책, 학습 콘텐츠 요약, 모델 평가, adversarial testing, systemic risk 완화, serious incident 보고, cybersecurity 보호 같은 구체 의무를 부과합니다.

가장 중요한 해석 포인트는 “frontier model deployer”라는 표현이 EU AI Act의 핵심 법정 용어가 아니라는 점입니다. EU AI Act는 “provider”, “deployer”, “downstream provider”, “general-purpose AI model”, “general-purpose AI model with systemic risk”를 구분합니다. 프런티어 모델을 개발·시장 출시하는 조직은 보통 GPAI provider 또는 systemic-risk GPAI provider로 분석해야 하고, 단순히 모델을 자기 서비스에 사용하는 조직은 deployer 또는 downstream provider가 될 수 있습니다. 따라서 “frontier model deployer compliance”라는 실무 표현을 EU AI Act에 곧바로 대응시키면 역할별 의무가 섞일 위험이 있습니다.

확인된 사실과 불확실성을 분리하면 다음과 같습니다. 확인된 사실은 2025년 8월 2일부터 EU의 GPAI provider 의무가 적용되기 시작했고, 2026년 8월 2일부터 Commission의 집행 권한과 과징금 집행이 본격화되며, 2025년 8월 2일 전에 이미 시장에 나온 GPAI 모델은 2027년 8월 2일까지 준수해야 한다는 점입니다. 또 10^25 FLOP을 초과하는 학습 연산량은 systemic risk GPAI의 high-impact capability 추정 기준으로 확인됩니다. 불확실성은 이 기준이 영구 고정값이 아니라 Article 51상 delegated act로 조정될 수 있고, Commission factpage도 해당 threshold가 검토 중이라고 밝힌다는 점입니다. 또한 2026년 5월 15일 기준으로 공개 공식 자료만으로는 모든 frontier 모델별 지정·통지 상태나 각 provider의 실제 준수 수준을 확정할 수 없습니다.

### 비교표

| 비교 항목 | NIST AI RMF | EU AI Act GPAI 의무 | frontier model deployer에 대한 의미 |
|---|---|---|---|
| 법적 성격 | 자발적 프레임워크 | EU Regulation 2024/1689의 구속적 법규 | NIST 채택만으로 EU 준수는 아님 |
| 대상 역할 | AI actors 전반: 설계, 개발, 배포, 운영, 평가 관련 조직 | 주로 GPAI model provider 및 systemic-risk GPAI provider | “deployer”인지 “provider”인지 먼저 분류해야 함 |
| 위험관리 구조 | GOVERN, MAP, MEASURE, MANAGE | Article 53, 55, 88, 101 등 법정 의무 | NIST 프로세스는 EU 증빙 체계의 운영 기반으로 유용 |
| 문서화 | 위험관리 문서화 권고 | 기술문서, downstream 정보, public training-content summary | EU에서는 특정 산출물이 요구됨 |
| 위험 허용도 | NIST는 risk tolerance를 정하지 않음 | systemic risk 여부와 법정 의무가 기준을 설정 | 자체 리스크 기준만으로 법정 의무를 대체할 수 없음 |
| 고급·프런티어 모델 | GenAI Profile이 GAI 위험과 suggested actions 제시 | 10^25 FLOP 추정, Commission 지정, Article 55 추가 의무 | 고성능 모델은 EU에서 별도 systemic-risk 검토 필요 |
| 집행·제재 | NIST 자체 제재 없음 | Commission/AI Office 감독, Article 101 과징금 | 준수 실패는 법적·재무적 리스크로 전환 |
| 해석 충돌 지점 | 유연하고 조직별 적용 | 표준·Code·가이드라인을 통한 구체화 | NIST의 “good practice”와 EU의 “legal obligation”을 구분해야 함 |

### 배경과 현재 맥락
NIST AI RMF는 2023년 1월 26일 발표된 미국 NIST의 위험관리 프레임워크입니다. AI가 개인, 조직, 사회에 미칠 수 있는 위험을 다루되, 특정 산업이나 사용 사례에 묶이지 않는 범용 프레임워크로 설계되었습니다. 2024년 7월 26일에는 생성형 AI에 특화된 NIST AI 600-1 Generative AI Profile이 공개되어, CBRN 정보, confabulation, 정보무결성, 개인정보, 지식재산, 정보보안 등 생성형 AI 특유 또는 증폭 위험을 체계화했습니다.

EU AI Act는 2024년 Regulation (EU) 2024/1689로 성립한 법적 규제 체계입니다. GPAI 모델 의무는 Chapter V에 있으며, Article 53은 모든 GPAI provider의 기본 의무, Article 55는 systemic-risk GPAI provider의 추가 의무, Article 88은 Commission의 감독·집행 권한, Article 101은 GPAI provider 과징금을 규정합니다. Commission은 2025년 GPAI Guidelines, public summary template, GPAI Code of Practice를 통해 적용 범위와 준수 기대치를 구체화했고, 2026년 4월 28일 업데이트된 안내에서도 2025년·2026년·2027년 단계별 일정을 확인할 수 있습니다.

### 확인된 핵심 사실
1. NIST AI RMF는 법적 의무가 아니라 자발적 위험관리 프레임워크입니다. 공식 NIST 페이지와 NIST AI 100-1은 voluntary, rights-preserving, non-sector-specific, use-case agnostic 성격을 확인합니다.

2. NIST AI RMF의 Core는 GOVERN, MAP, MEASURE, MANAGE입니다. 이는 조직의 거버넌스, 맥락 파악, 위험 측정, 위험 관리를 반복적으로 연결하는 구조입니다.

3. NIST AI RMF는 위험 허용도 자체를 정하지 않습니다. NIST는 risk tolerance가 법·규제·조직·분야별 기준에 따라 달라진다고 보고, 기존 법규와 규범에 맞춰 정해야 한다고 설명합니다.

4. EU AI Act Article 53은 GPAI provider에게 기술문서 작성·유지, downstream AI system provider에 대한 정보 제공, EU 저작권법 준수 정책, 학습 콘텐츠 요약 공개를 요구합니다.

5. EU AI Act Article 55는 systemic-risk GPAI provider에게 모델 평가, adversarial testing, systemic risk 평가·완화, serious incident 보고, cybersecurity 보호를 추가로 요구합니다.

6. EU AI Act상 10^25 FLOP 초과 학습 연산량은 high-impact capability 추정 기준입니다. 그러나 Article 51은 Commission이 기술 발전에 따라 threshold와 benchmark를 delegated act로 수정할 수 있다고 규정하고, Commission factpage도 이 threshold가 현재 검토 중이라고 밝힙니다.

7. Code of Practice는 자발적 도구이지만 Commission과 AI Board가 적정한 준수 입증 수단으로 인정했습니다. Code에 서명하지 않는 provider는 alternative adequate means of compliance를 제시해야 합니다.

8. 2026년 5월 15일 기준, 이 보고서는 정보 보고서이며 법률 자문이 아닙니다. 실제 의무 판단은 역할, 시장 출시 방식, 모델 접근 방식, 오픈소스 조건, fine-tuning 정도, EU 시장 제공 여부에 따라 달라집니다.

### 요구사항·해석 충돌
첫 번째 충돌은 “자발적 위험관리”와 “법적 준수”의 혼동입니다. NIST AI RMF는 충분히 좋은 내부 통제·평가·문서화 기반이 될 수 있지만, EU AI Act Article 53·55의 특정 산출물과 통지·보고·저작권·정보제공 의무를 자동 충족하지 않습니다.

두 번째 충돌은 “deployer”라는 표현입니다. NIST는 AI actors와 deployers를 넓게 다루지만, EU AI Act에서 deployer는 “AI system을 자기 권한 아래 사용하는 자”입니다. GPAI 모델 의무는 주로 provider에게 부과됩니다. 따라서 frontier model을 “배포한다”는 일상 표현이 법적으로는 placing on the market을 하는 provider인지, AI system에 통합하는 downstream provider인지, 단순 사용자인 deployer인지 분해되어야 합니다.

세 번째 충돌은 10^25 FLOP 기준의 안정성입니다. 현재 법령상 추정 기준은 확인되지만, Commission의 수정 권한과 검토 중이라는 공식 고지가 있으므로 장기 기준으로 고정해 compliance architecture를 설계하면 위험합니다. 실무적으로는 현재 10^25 FLOP 기준을 기준선으로 삼되, delegated act와 AI Office 안내 변경을 추적해야 합니다.

### 대표 사례와 실무 적용
대표 사례 1은 대형 foundation model을 EU 시장에 API 또는 다운로드 방식으로 제공하는 조직입니다. 이 조직은 EU AI Act상 GPAI provider가 될 가능성이 높고, 모델이 10^25 FLOP 추정 기준 또는 Commission 지정 기준에 해당하면 systemic-risk GPAI provider로 Article 55 의무까지 검토해야 합니다. NIST AI RMF는 여기서 governance, risk mapping, model evaluation, incident response 절차 설계에 도움을 주지만, EU Article 53 문서·요약·저작권 정책 산출물을 별도로 맞춰야 합니다.

대표 사례 2는 외부 frontier model을 받아 자체 애플리케이션에 통합하는 SaaS 기업입니다. 이 기업은 단순 모델 provider가 아니라 downstream provider 또는 AI system provider로 분석될 수 있습니다. 이 경우 핵심은 base model provider로부터 Article 53 기반 정보를 확보하고, 자체 시스템이 high-risk AI system인지 또는 transparency obligation 대상인지 별도로 검토하는 것입니다.

대표 사례 3은 오픈소스 GPAI 모델입니다. EU AI Act는 일정 조건의 free and open-source GPAI model에 Article 53(1)(a), (b) 문서 의무 예외를 둡니다. 그러나 Article 53(1)(c), (d)의 저작권 정책과 학습 콘텐츠 요약, 그리고 systemic-risk 모델 예외 배제는 남습니다. 즉 “오픈소스이므로 EU GPAI 의무가 없다”는 해석은 과도합니다.

### 남아 있는 열린 쟁점
공식 자료로 확인 가능한 범위에서 가장 큰 미해결 쟁점은 모델별 지정·통지·준수 상태입니다. Commission은 notification contact와 제출 절차를 안내하지만, 모든 systemic-risk GPAI 모델의 공개 목록이나 각 provider의 준수 수준이 완전하게 공개되어 있다고 확인되지는 않습니다. 두 번째 열린 쟁점은 10^25 FLOP 기준의 향후 변경입니다. 세 번째는 Code of Practice와 harmonised standards의 관계입니다. Article 55는 harmonised standard가 나오기 전 code of practice 활용을 허용하고, 향후 표준이 나오면 presumption of conformity 구조가 달라질 수 있습니다.

### 실무 권고
frontier model deployer라는 표현을 쓰는 조직은 먼저 EU AI Act 역할 매핑표를 만들어야 합니다: model provider인지, downstream provider인지, AI system provider인지, deployer인지, authorized representative가 필요한지 구분해야 합니다. 그 다음 NIST AI RMF의 GOVERN-MAP-MEASURE-MANAGE를 내부 운영 체계로 두고, EU AI Act Article 53·55 산출물을 별도 compliance evidence register로 관리하는 방식이 가장 안전합니다. 특히 systemic-risk 가능성이 있는 모델은 10^25 FLOP, benchmark, capability, EU 시장 reach, downstream impact, serious incident reporting 절차를 별도 트랙으로 운영해야 합니다. 다만 이 권고는 정보 보고서 수준이며, 특정 회사의 법적 의무 확정에는 EU counsel 및 AI Office 최신 가이드 확인이 필요합니다.

# 검증 부록

## 출처 감사 (Source Audit)
| ID | URL | Source | Class | Checked Fact | Limitation | Diagnostics |
| --- | --- | --- | --- | --- | --- | --- |
| S1 | https://www.nist.gov/itl/ai-risk-management-framework | AI Risk Management Framework | primary_official | NIST AI RMF is voluntary, rights-preserving, non-sector-specific, and use-case agnostic. | Summary webpage; detailed framework claims checked against NIST AI 100-1 PDF and publication page. | - |
| S2 | https://www.nist.gov/publications/artificial-intelligence-risk-management-framework-ai-rmf-10 | Artificial Intelligence Risk Management Framework (AI RMF 1.0) | primary_official | NIST AI RMF is voluntary, rights-preserving, non-sector-specific, and use-case agnostic. | Publication landing page; detailed claims checked with PDF. | - |
| S3 | https://nvlpubs.nist.gov/nistpubs/ai/NIST.AI.100-1.pdf | Artificial Intelligence Risk Management Framework (AI RMF 1.0), NIST AI 100-1 | primary_official_pdf | NIST AI RMF is voluntary, rights-preserving, non-sector-specific, and use-case agnostic. | Not a binding legal compliance regime. | - |
| S4 | https://nvlpubs.nist.gov/nistpubs/ai/NIST.AI.600-1.pdf | Artificial Intelligence Risk Management Framework: Generative Artificial Intelligence Profile, NIST AI 600-1 | primary_official_pdf | NIST AI 600-1 identifies GAI-specific or GAI-exacerbated risks and suggested actions. | Profile guidance; not EU legal compliance. | - |
| S5 | https://eur-lex.europa.eu/eli/reg/2024/1689/oj/eng | Regulation (EU) 2024/1689 | primary_official_law | EU AI Act official legal instrument | Used with accessible official article pages for detailed extraction. | - |
| S6 | https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-3 | Article 3: Definitions | primary_official_service_desk | EU AI Act provider and deployer are separate legal roles, and GPAI model obligations are provider-centered. | Service Desk summaries are non-binding; article text states it uses official Regulation version. | - |
| S7 | https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-51 | Article 51: Classification of general-purpose AI models as general-purpose AI models with systemic risk | primary_official_service_desk | Training compute greater than 10^25 FLOP is a presumption threshold for high-impact capabilities under Article 51. | Threshold may change. | - |
| S8 | https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-53 | Article 53: Obligations for providers of general-purpose AI models | primary_official_service_desk | EU AI Act provider and deployer are separate legal roles, and GPAI model obligations are provider-centered. | Service Desk summary is non-binding. | - |
| S9 | https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-55 | Article 55: Obligations of providers of general-purpose AI models with systemic risk | primary_official_service_desk | EU AI Act provider and deployer are separate legal roles, and GPAI model obligations are provider-centered. | Standards landscape may evolve. | - |
| S10 | https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-88 | Article 88: Enforcement of the obligations of providers of general-purpose AI models | primary_official_service_desk | The Commission has exclusive powers to supervise and enforce Chapter V and entrusts implementation to the AI Office. | Operational enforcement practice still developing. | - |
| S11 | https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-101 | Article 101: Fines for providers of general-purpose AI models | primary_official_service_desk | Article 101 provides GPAI provider fines up to 3% worldwide annual turnover or EUR 15 million, whichever is higher. | Detailed procedural implementing acts may affect practice. | - |
| S12 | https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-113 | Article 113: Entry into force and application | primary_official_service_desk | GPAI obligations apply from 2025-08-02, Commission enforcement powers from 2026-08-02, and pre-2025-08-02 GPAI models must comply by 2027-08-02. | Must be read with Commission timeline guidance. | - |
| S13 | https://digital-strategy.ec.europa.eu/en/factpages/general-purpose-ai-obligations-under-ai-act | General-purpose AI obligations under the AI Act | primary_official | Training compute greater than 10^25 FLOP is a presumption threshold for high-impact capabilities under Article 51. | Guidance/factpage, not legal text. | - |
| S14 | https://digital-strategy.ec.europa.eu/en/policies/guidelines-gpai-providers | Guidelines for providers of general-purpose AI models | primary_official_guidance | GPAI obligations apply from 2025-08-02, Commission enforcement powers from 2026-08-02, and pre-2025-08-02 GPAI models must comply by 2027-08-02. | Interpretive guidance. | - |
| S15 | https://digital-strategy.ec.europa.eu/en/library/commission-opinion-assessment-general-purpose-ai-code-practice | Commission Opinion on the assessment of the General-Purpose AI Code of Practice | primary_official | The GPAI Code of Practice is voluntary but can be used as an adequate tool to demonstrate compliance. | Does not itself prove individual provider compliance. | - |
| S16 | https://digital-strategy.ec.europa.eu/en/news/eu-rules-general-purpose-ai-models-start-apply-bringing-more-transparency-safety-and-accountability | EU rules on general-purpose AI models start to apply, bringing more transparency, safety and accountability | primary_official_press | GPAI obligations apply from 2025-08-02, Commission enforcement powers from 2026-08-02, and pre-2025-08-02 GPAI models must comply by 2027-08-02. | Press release, not legal text. | - |
| S17 | https://digital-strategy.ec.europa.eu/en/policies/signatory-taskforce-gpai-code-practice | Signatory Taskforce of the General-Purpose AI Code of Practice | primary_official | Signatory Taskforce established | Does not establish compliance of individual providers. | - |
| S18 | https://www.nist.gov/artificial-intelligence/executive-order-safe-secure-and-trustworthy-artificial-intelligence | Executive Order on Safe, Secure, and Trustworthy Artificial Intelligence | primary_official | EO 14110 was rescinded on January 20 2025 | Context only; NIST AI RMF and AI 600-1 remain accessible publications. | - |
## 주장 로그 (Claim Log)
| ID | Claim | Support | Confidence | Uncertainty |
| --- | --- | --- | --- | --- |
| C1 | NIST AI RMF is voluntary, rights-preserving, non-sector-specific, and use-case agnostic. | S1 (https://www.nist.gov/itl/ai-risk-management-framework); S2 (https://www.nist.gov/publications/artificial-intelligence-risk-management-framework-ai-rmf-10); S3 (https://nvlpubs.nist.gov/nistpubs/ai/NIST.AI.100-1.pdf) | high | - |
| C2 | NIST AI RMF Core consists of GOVERN, MAP, MEASURE, and MANAGE. | S3 (https://nvlpubs.nist.gov/nistpubs/ai/NIST.AI.100-1.pdf) | high | - |
| C3 | NIST AI RMF does not prescribe risk tolerance. | S3 (https://nvlpubs.nist.gov/nistpubs/ai/NIST.AI.100-1.pdf) | high | - |
| C4 | NIST AI 600-1 identifies GAI-specific or GAI-exacerbated risks and suggested actions. | S4 (https://nvlpubs.nist.gov/nistpubs/ai/NIST.AI.600-1.pdf) | high | - |
| C5 | EU AI Act provider and deployer are separate legal roles, and GPAI model obligations are provider-centered. | S6 (https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-3); S8 (https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-53); S9 (https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-55) | high | - |
| C6 | Article 53 imposes technical documentation, downstream provider information, copyright policy, and public training-content summary obligations on GPAI providers. | S8 (https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-53) | high | - |
| C7 | Article 55 imposes model evaluation, adversarial testing, systemic-risk mitigation, serious incident reporting, and cybersecurity obligations on systemic-risk GPAI providers. | S9 (https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-55) | high | - |
| C8 | Training compute greater than 10^25 FLOP is a presumption threshold for high-impact capabilities under Article 51. | S7 (https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-51); S13 (https://digital-strategy.ec.europa.eu/en/factpages/general-purpose-ai-obligations-under-ai-act) | high | - |
| C9 | The 10^25 FLOP threshold may be amended and is under review, so it should not be treated as permanently fixed. | S7 (https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-51); S13 (https://digital-strategy.ec.europa.eu/en/factpages/general-purpose-ai-obligations-under-ai-act) | high | - |
| C10 | GPAI obligations apply from 2025-08-02, Commission enforcement powers from 2026-08-02, and pre-2025-08-02 GPAI models must comply by 2027-08-02. | S12 (https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-113); S14 (https://digital-strategy.ec.europa.eu/en/policies/guidelines-gpai-providers); S16 (https://digital-strategy.ec.europa.eu/en/news/eu-rules-general-purpose-ai-models-start-apply-bringing-more-transparency-safety-and-accountability) | high | - |
| C11 | The GPAI Code of Practice is voluntary but can be used as an adequate tool to demonstrate compliance. | S15 (https://digital-strategy.ec.europa.eu/en/library/commission-opinion-assessment-general-purpose-ai-code-practice); S16 (https://digital-strategy.ec.europa.eu/en/news/eu-rules-general-purpose-ai-models-start-apply-bringing-more-transparency-safety-and-accountability) | high | - |
| C12 | The Commission has exclusive powers to supervise and enforce Chapter V and entrusts implementation to the AI Office. | S10 (https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-88) | high | - |
| C13 | Article 101 provides GPAI provider fines up to 3% worldwide annual turnover or EUR 15 million, whichever is higher. | S11 (https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-101) | high | - |
| C14 | NIST AI RMF can support EU AI Act compliance operations but does not itself establish EU legal compliance. | S3 (https://nvlpubs.nist.gov/nistpubs/ai/NIST.AI.100-1.pdf); S8 (https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-53); S9 (https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-55); S11 (https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-101) | high | - |
| C15 | The phrase frontier model deployer is ambiguous under EU AI Act and must be mapped to provider, deployer, downstream provider, or AI system provider roles. | S6 (https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-3); S8 (https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-53); S9 (https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-55); S14 (https://digital-strategy.ec.europa.eu/en/policies/guidelines-gpai-providers) | medium-high | - |
## 한계, 충돌, 연구 부채 (Limits, Conflicts, And Research Debt)
### Unresolved Conflicts
- K3: Stability of 10^25 FLOP threshold [Current presumption threshold is verified; amendment power and official under-review status are verified limitations.]
### Research Debt
- D1 [open]: Latest delegated act or official confirmation on whether the 10^25 FLOP threshold has changed after the reviewed Commission factpage. | next=Check EUR-Lex for delegated acts under Article 51 and Article 97.; Check AI Office and Commission digital-strategy updates for threshold-review announcements.
- D2 [open]: Actual enforcement examples or implementing procedural acts for Article 101 GPAI fines. | next=Check Commission implementing acts under Article 101(6).; Monitor AI Office enforcement announcements from 2 August 2026 onward.
- D3 [open]: Official public list of GPAI models designated or notified as systemic risk. | next=Check AI Office resources and press releases.; Check EUR-Lex and Commission decisions for Article 51 designations.
- D4 [open]: Provider-level compliance assessment results for GPAI Code signatories. | next=Review taskforce minutes and public summaries.; Check AI Office compliance assessment publications after enforcement starts.
- debt-conflict-map-contains-1-unresolved-conflict-s-not [open]: conflict map contains 1 unresolved conflict(s) not promoted to debt | next=Acquire or cite stronger evidence tied to Source Card IDs or full URLs.; Resolve the specific failed artifact gate before broad rewriting.
- debt-research-artifact-gate-failed-conflict-map-contains-1 [open]: Research artifact gate failed: conflict map contains 1 unresolved conflict(s) not promoted to debt | next=Repair the failed quality gate item with stronger evidence or a narrower claim.
### Target Host / Source Class Misses
- query=`Compare the NIST AI Risk Management Framework with the EU AI Act compliance obligations for f nist.gov official guidance` status=missed expected_host=nist.gov expected_source_class=official_or_primary provider=naver reason=Official-host hint query did not recover target domain nist.gov.
## 품질 게이트 (Quality Gate)
| Check | Result | Note |
| --- | --- | --- |
| deterministic gate status | passed | artifact-backed finalization rendered visible verification sections before validation |
| failure messages | pass | none |
| unsupported claim count | 0 | lower is better |
| unresolved conflict count | 0 | unresolved items must stay visible as debt or limits |
| open debt count | 6 | open debt never counts as acceptance |
