# Cheeky Foveated DLSS — pesquisa e arquitetura de receitas assistidas por IA

## Decisão de produto

Cheeky Foveated DLSS não deve ser modelado como uma cópia genérica de um único `.addon64`. A receita precisa decidir primeiro se o jogo é elegível, qual rota de integração é válida, quais pré-requisitos já existem, qual perfil pode ser aplicado com segurança e como o resultado será validado. A IA pode ajudar a montar uma proposta, interpretar diagnósticos e registrar evidências, mas não deve escolher uma rota incompatível, inventar valores ou executar alterações fora de uma lista permitida.

A versão corrente publicada pelo upstream é a `v0.3.4`. Ela mantém três artefatos distintos: o add-on do ReShade, o pacote completo para UEVR e o instalador compartilhado da camada OpenXR. Cada artefato possui destino, atualização e rollback próprios.[^1]

| Jogo no Moddin | Elegibilidade técnica | Evidência Cheeky específica | Estado recomendado |
| --- | --- | --- | --- |
| Cyberpunk 2077 | Tem DLSS Super Resolution nativo; ainda requer detecção da API, ReShade com add-ons e da rota VR real quando usada. | Não consta na lista upstream de jogos testados; o autor o descreveu como não testado em discussão da comunidade.[^11] | `experimental` com teste local controlado. |
| Elden Ring | O jogo base não deve ser tratado como provedor de DLSS. Uma receita só pode prosseguir se detectar um provedor explícito de DLSS, por exemplo ERSS-FG ou equivalente. | Não consta na lista upstream de jogos testados. A cadeia “mod de DLSS → ReShade → Cheeky” amplia a superfície de falha. | `blocked` sem provedor; `experimental` apenas quando o provedor for validado. |

O upstream cita Forza Horizon 6, Red Dead Redemption 2, Assetto Corsa Competizione, Stellar Blade Demo e Hogwarts Legacy via UEVR como testados anteriormente, mas diz expressamente que compatibilidade varia por build e integração.[^2] Isso é evidência de ponto de partida, não uma garantia reutilizável para outro jogo.

## Limites técnicos confirmados

O Cheeky requer Windows 10/11 de 64 bits, GPU NVIDIA RTX, jogo D3D11 ou D3D12 com DLSS Super Resolution e uma única integração: ReShade com suporte completo a add-ons **ou** UEVR com API de plugin compatível. ReShade e o plugin UEVR não podem ser carregados juntos no mesmo jogo.[^2]

Há quatro rotas reais:

| Rota | Destino da instalação | Pré-requisitos | Regra de exclusão |
| --- | --- | --- | --- |
| `reshade_desktop` | `CheekyFoveatedDLSS.addon64` ao lado do executável e da DLL do ReShade | ReShade 64-bit com suporte completo a add-ons; D3D11/D3D12; DLSS-SR ativo | Não usar UEVR Cheeky. |
| `reshade_openxr` | Igual à rota desktop | Tudo da rota ReShade mais `CheekyOpenXRSetup.exe` correspondente | A camada OpenXR é global e deve ter a mesma versão do Cheeky. |
| `uevr_openxr` | ZIP completo em `%APPDATA%\UnrealVRMod\<executável>` | UEVR API 2.39.0 ou 2.x compatível; camada OpenXR correspondente | Remover ReShade e o add-on Cheeky do diretório do jogo. |
| `native_openvr` | ReShade ou UEVR conforme a rota escolhida | D3D11/D3D12, DLSS-SR e rota válida | Não requer instalador OpenXR para a calibração nativa. |

O ReShade possui uma edição oficial com suporte completo a add-ons; essa edição é descrita pelo próprio ReShade como não assinada. Por isso, a instalação ou troca de ReShade deve permanecer uma ação explícita, com origem oficial e confirmação do usuário, e não uma decisão automática da IA.[^3]

## Escada de evidência

Toda afirmação de compatibilidade deve guardar a origem e o escopo. Não misturar relato comunitário com teste reproduzível evita que uma configuração “funcionou para alguém” vire regra global.

| Nível | Origem mínima | Uso permitido pela IA | Não permite concluir |
| --- | --- | --- | --- |
| `upstream_confirmed` | Jogo e integração citados pelo upstream | Sugerir perfil-base conservador | Compatibilidade com outra versão, headset ou rota. |
| `local_reproduced` | Teste local com versão, rota, diagnóstico e comparação A/B | Mostrar “funcionou neste PC” e reutilizar somente na mesma versão | Transformar em compatível para todos. |
| `community_reported` | Relato público sem diagnóstico completo | Priorizar perguntas e testes; nunca autoaplicar perfil agressivo | Considerar provado. |
| `risk_reported` | Issue/reprodução de congelamento, crash, regressão ou incompatibilidade | Elevar alerta e criar gate de confirmação | Declarar falha universal. |
| `unknown` | Sem evidência específica | Manter bloqueado ou experimental | Sugerir parâmetros personalizados. |

Há riscos concretos no rastreador upstream: Crimson Desert registra imagem congelada quando usa recorte parcial via caminho Streamline; Blood of Dawnwalker registra crash no momento da injeção UEVR; AFW é reportado como incompatível com o modelo atual de rastreamento ocular; e há uma notificação de possível aumento de uso de GPU em Banishers com a versão 0.3.4 do plugin UEVR.[^4] Esses casos devem virar regras de diagnóstico, não “listas negras” permanentes.

## Fluxo determinístico de uma receita

```text
inventário local
  -> verificação de elegibilidade
  -> escolha exclusiva da rota
  -> plano de arquivos com backup
  -> pré-configuração segura
  -> execução manual do jogo
  -> calibração/diagnóstico dentro do jogo
  -> benchmark A/B e observação visual
  -> classificação local e evidência versionada
  -> rollback ou promoção controlada do perfil
```

Cada seta deve persistir resultado. A IA não deve pular de “executável encontrado” para “configuração provada”.

## Segmentos que a IA deve receber

### 1. Identidade, confiança e versão

Campos imutáveis: `game_id`, Steam App ID, caminho relativo do executável, arquitetura, versão do Cheeky, URL oficial, SHA-256, tipo de artefato e versão do provedor de DLSS. O executor aceita download apenas do release oficial e valida hash antes da cópia.

Para atualizações, a IA precisa saber qual artefato será trocado. Em `v0.3.4`, o ReShade troca apenas o add-on; o UEVR requer ZIP completo, com `plugins/`, `scripts/` e DLLs atualizados; OpenXR exige executar o instalador correspondente.[^1]

### 2. Descoberta do ambiente

O probe deve produzir fatos, e não inferências textuais:

| Grupo | Campos mínimos | Consequência |
| --- | --- | --- |
| Executável | existe, 64-bit, processo em execução, diretório real | Processo ativo bloqueia mutação. |
| Renderização | API observada D3D11/D3D12, DLL proxy do ReShade, logs disponíveis | API ausente ou diferente bloqueia a rota. |
| DLSS | DLSS-SR habilitável/observado, `nvngx`/Streamline, provedor externo identificado | Sem DLSS-SR nativo ou provedor validado, Cheeky não é configurado. |
| Integração | ReShade com add-ons, UEVR e versão da API, presença de Cheeky anterior | Seleciona exatamente uma rota e detecta conflito. |
| VR | desktop, OpenVR, OpenXR, OpenComposite; runtime ativo; eye tracking real | Determina se a camada OpenXR é obrigatória e se gaze é permitido. |
| Hardware | RTX, driver, GPU, resolução, HDR, headset | Contextualiza limites; não basta para marcar como compatível. |
| Segurança | anti-cheat/online, permissões, overlays e injetores detectados | Exige confirmação ou bloqueio definido pela receita. |

O diagnóstico do próprio Cheeky diferencia a interceptação de DLSS, resoluções recebidas, crop, contagem de chamadas, tempo de GPU e último resultado NGX. “Aguardando a primeira avaliação DLSS” significa que a IA deve orientar a habilitação de DLSS e conferir se o ReShade foi instalado para a API correta, não alterar os sliders.[^5]

### 3. Seletor de rota

O seletor é uma máquina de estados pequena:

```text
if process_running                 => blocked(game_running)
else if !x64 || !rtx               => blocked(platform)
else if !d3d11 && !d3d12           => blocked(renderer)
else if !dlss_sr_provider           => blocked(dlss_missing)
else if reshade_addon && uevr       => needs_choice(conflicting_integrations)
else if uevr_selected && !uevr_api  => blocked(uevr_api)
else if openxr && !layer_version    => prerequisite(openxr_layer)
else                                => ready(route)
```

`dlss_sr_provider` deve ser um objeto, não um booleano: `native`, `erss_fg`, `optiscaler`, `other_verified` ou `unknown`. Para Elden Ring, `unknown` bloqueia a aplicação; uma presença de DLL por si só não prova que DLSS-SR esteja exposto ao Cheeky.

### 4. Plano de arquivo e rollback

Separar ações mutáveis simplifica segurança e auditoria:

| Script/ação | Pode alterar | Backup obrigatório | Verificação pós-ação |
| --- | --- | --- | --- |
| `install_reshade_addon` | Somente `.addon64` e marcador Moddin | Add-on anterior e marcador | Hash, caminho ao lado do executável, versão do marcador. |
| `install_uevr_package` | Arquivos declarados do ZIP dentro da configuração UEVR | Cada arquivo sobrescrito e diretório criado | Estrutura `plugins/` + `scripts/`, versões e ausência de add-on ReShade Cheeky. |
| `install_openxr_layer` | Instalador upstream no escopo do Windows | Estado anterior do instalador/versão registrado | Camada registrada e versão compatível. |
| `write_profile` | Arquivo de configurações explicitamente declarado | Arquivo inteiro | Schema, grupo e limites validados. |
| `rollback` | Apenas alvos presentes na transação | Não sobrescreve mudanças externas não pertencentes à transação | Hash/restauração e re-probe. |

O pacote UEVR não é um arquivo para copiar perto do `.exe`; o upstream determina a configuração UEVR por jogo. A camada OpenXR também não pode ser “instalada” copiando uma DLL: ela precisa de registro pelo instalador.[^1][^6]

## Perfis de configuração

Os campos persistidos pelo upstream usam o grupo `[CheekyFoveatedDLSS]` e schema `1`. O código upstream separa os campos em `sr`, `gaze` e `nr`; o Moddin deve preservar essa mesma separação para evitar que uma recomendação de qualidade altere calibração estereoscópica.[^7]

### Perfil `sr_safe`

Aplicável quando DLSS-SR já foi confirmado no jogo, antes de qualquer otimização por IA.

| Campo | Valor inicial | Limite permitido | Regra da IA |
| --- | --- | --- | --- |
| `Enabled` | `true` | booleano | Não ativar até o diagnóstico ver DLSS-SR. |
| `PeripheralDlaa` | `true` | booleano | Manter ligado no primeiro teste. |
| `PeripheralDlaaScale` | `0.75` | `0.20–1.00` | Reduzir somente após A/B limpo. |
| `CenterPreset` | `0` (do jogo) | `0`, `5`, `11`, `12`, `13` | Não escolher preset por heurística genérica. |
| `CenterSupersampling` | `1.00` | `1.00–2.00` | Aumentar apenas quando a qualidade central for medida como insuficiente. |
| `PeripheralDlaaPreset` | `5` | `5`, `11`, `12`, `13` | Não alterar no primeiro perfil. |
| `Width` / `Height` | `0.55` / `0.45` | `0.20–1.00` | Diminuir em passos pequenos, com inspeção visual. |
| `Roundness` | `0.00` | `0.00–1.00` | Preferência visual; não alegar ganho de desempenho. |
| `TransitionWidth` | `0.040` | `0.00–0.30` | Ajustar para esconder transição, não para buscar FPS. |

Os defaults e as faixas foram confirmados na referência de uso e na validação de settings do upstream.[^5][^7]

### Perfil `vr_alignment_safe`

Aplicável apenas quando a rota VR e a calibração foram verificadas.

| Campo | Valor inicial | Regra |
| --- | --- | --- |
| `CenterMode` | `fixed` | Usar gaze somente depois que runtime/headset informar gaze utilizável. |
| `AutoStereoAlignment` | `true` | Nunca desligar automaticamente. |
| `AlignedHeightOffset` | `0.00` | Alterar apenas após border e diagnósticos. |
| `AlignmentBorder` | `true` temporariamente | Aceitar somente após sobreposição visual total; desligar depois. |
| `GazeSmoothingMs` | `20` | Manter no primeiro teste. |
| `GazeQuantizationPixels` | `8` | Manter no primeiro teste. |
| `GazeJumpResetRatio` | `0.125` | Não personalizar sem medição de estabilidade/histórico. |

Em OpenXR, a camada correspondente é obrigatória mesmo sem headset com eye tracking. A calibração automática precisa continuar ativa; offsets e inversão de olho são recursos de diagnóstico, pois a ordem das vistas pode mudar entre cenas.[^2][^6]

### Perfil `nr_experimental`

DLSS-NR é um ramo separado e fica desativado por padrão. A receita só pode expô-lo depois de validar runtimes NVIDIA/Streamline compatíveis e a presença do `nvngx_dlssnr.dll` assinado. D3D11 exige `D3D11D3D12Transport` para NR, e RDR2 é apontado pelo upstream como caso que não funciona bem com NR.[^5]

| Campo | Valor inicial | Limite | Gate |
| --- | --- | --- | --- |
| `NrEnabled` | `false` | booleano | Confirmação explícita + runtimes validados. |
| `NrFoveated` | `true` | booleano | Só após SR ou geometria NR independente validada. |
| `NrProcessingOrder` | `after` (`0`) | `after`/`before` | Comparação A/B obrigatória; não há ganho medido universal. |
| `NrWorkingScale` | `1.00` | `0.10–1.00` | Reduzir em perfil de teste, nunca no perfil seguro. |
| `NrIntensity` | `1.00` | `0.00–1.00` | Valores acima de 1 não trazem efeito nesse runtime medido. |
| `NrStyle` | `standard` (`0`) | `0–2` | Registrar estilo e cenas usadas na comparação. |
| `NrLocal*Strength` | `1.00` | `0.00–2.00` | Alterar uma variável por teste. |
| `NrAutomaticMask` | `false` | booleano | Só então liberar `NrSkinStructureStrength`. |

O próprio upstream mediu alguns parâmetros no runtime analisado, mas declara que tais medições não provam compatibilidade de cada jogo/runtime. A IA deve tratar isso como limites de segurança, não como promessa visual.[^8]

## Contrato de saída da IA

A IA deve retornar uma proposta validável. Ela não deve retornar comandos de shell arbitrários nem um arquivo INI completo sem diff.

```yaml
schema_version: 1
game:
  id: cyberpunk-2077
  executable: bin/x64/Cyberpunk2077.exe
  executable_hash: "optional-observed-hash"
observed_environment:
  architecture: x64
  renderer: d3d12
  dlss_provider:
    kind: native # native | erss_fg | optiscaler | other_verified | unknown
    evidence: in_game_diagnostics
  integration:
    reshade_addon_support: true
    uevr_api_version: null
  vr:
    mode: openxr # desktop | openvr | openxr | unknown
    eye_tracking: unavailable
decision:
  state: experimental # blocked | prerequisite | ready | experimental | proven_local | risky | not_working
  route: reshade_openxr
  confidence: medium
  reasons:
    - "DLSS-SR observed in diagnostics"
    - "Game absent from upstream tested list"
  required_user_confirmation:
    - install_or_change_reshade
    - enable_experimental_nr
file_plan:
  artifact: CheekyFoveatedDLSS.addon64
  source: official_release
  sha256: "release-specific-sha256"
  target: game_executable_directory
  transaction_kind: cheeky-foveated-dlss
profile:
  name: sr_safe
  write_groups: [sr]
  changes:
    Enabled: true
    PeripheralDlaa: true
    PeripheralDlaaScale: 0.75
    CenterSupersampling: 1.0
    Width: 0.55
    Height: 0.45
    TransitionWidth: 0.04
verification:
  preflight:
    - game_closed
    - hash_valid
    - no_conflicting_cheeky_integration
  runtime:
    - dlss_interception_active
    - first_evaluation_received
    - no_crash_or_freeze
  acceptance:
    - compare_same_scene_ab
    - record_fps_or_gpu_time
    - inspect_transition_and_artifacts
evidence_to_store:
  cheeky_version: 0.3.4
  provider_version: "observed-version"
  status: experimental
  notes: "human summary"
  diagnostic_zip_path: "optional-local-path"
```

O validador deve rejeitar: uma chave desconhecida, valor fora da faixa, grupo não permitido, rota dupla, arquivo de destino fora do diretório autorizado, `NrEnabled: true` sem gate, e `proven_local` sem uma sessão A/B registrada.

## Protocolo de teste e classificação

### Aceitação mínima

1. Com o jogo fechado, validar hash, arquivo de destino, backup e ausência da integração Cheeky conflitante.
2. No jogo, habilitar DLSS-SR e confirmar que o painel Cheeky recebeu pelo menos uma avaliação.
3. Usar uma cena repetível. Comparar Cheeky desligado e `sr_safe` com a mesma resolução, preset e modo DLSS.
4. Registrar FPS ou GPU time do pipeline, qualidade central, periferia, ghosting, flicker, freeze, crash e comportamento de menus.
5. Em VR, usar a borda de alinhamento e só aceitar a sessão se ambos os olhos estiverem alinhados. Para OpenXR, guardar o estado da camada e o diagnóstico de calibração.
6. Se houver erro, gerar o ZIP de suporte; ele contém settings, diagnósticos, versões de DLLs e logs, mas pode carregar caminhos/identificadores pessoais e não é enviado automaticamente.[^5]

### Resultado local

| Status | Critério mínimo | Próxima ação |
| --- | --- | --- |
| `unverified` | Sem sessão válida | Não sugerir tuning. |
| `experimental` | Instalação e interceptação funcionam, mas teste incompleto ou sem repetição | Oferecer somente `sr_safe`. |
| `proven_local` | A/B sem crash/freeze/artefato impeditivo, métricas registradas, mesma versão/rota | Reutilizar no mesmo PC; revalidar em atualização. |
| `risky` | Funciona com regressão visual, instabilidade, fallback ou requisito frágil | Exibir alerta e manter rollback visível. |
| `not_working` | Falha reproduzível, freeze, crash ou não intercepta apesar de pré-requisitos confirmados | Não reaplicar; oferecer coleta de diagnóstico e rollback. |

Um resultado sempre deve carregar `cheeky_version`, `game_build` quando disponível, `route`, `renderer`, `dlss_provider`, `vr_mode`, hardware relevante e data. Mudou a versão do Cheeky, provedor de DLSS, rota ou API? O status volta para `experimental` até novo teste.

## Aplicação aos jogos atuais

### Cyberpunk 2077

Cyberpunk tem suporte oficial a DLSS Super Resolution, inclusive opções modernas de DLSS no jogo.[^9] Isso o torna um candidato técnico melhor para a rota ReShade do que um jogo sem DLSS. Ainda assim, Cheeky não o lista entre os títulos testados e a rota VR depende do mod/runtime efetivamente selecionado. A receita recomendada é `reshade_desktop` ou `reshade_openxr` conforme o probe, sempre `sr_safe`, com prioridade para benchmark interno do jogo ou cena reprodutível.

Campos específicos adicionais:

- versão do jogo e modo DLSS escolhido;
- D3D12 observado pelo ReShade/diagnóstico;
- presença de VR Port e API VR efetiva, sem inferir OpenXR apenas porque SteamVR está aberto;
- overlays/injetores no diretório que possam conflitar;
- resultado do benchmark A/B e artefatos em path tracing, se usado.

### Elden Ring

Elden Ring precisa de uma receita em duas etapas. Primeiro, detectar e validar o provedor de DLSS; depois, considerar Cheeky. A documentação do ERSS-FG, por exemplo, orienta selecionar DLSS/FSR/XeSS em seu menu, o que torna esse provedor um pré-requisito observável, não uma suposição.[^10]

Campos específicos adicionais:

- `dlss_provider.kind` e sua versão;
- confirmação dentro do menu do provedor de que DLSS-SR está ativo;
- cadeia de proxy DLLs e conflitos com ReShade;
- rota offline já exigida por ERVR/mods, sem qualquer tentativa do Moddin de burlar anti-cheat;
- ReShade com add-ons e Cheeky somente depois da confirmação do provedor.

O estado inicial correto para Elden Ring é `blocked(dlss_provider_missing)`, não apenas `experimental`. Quando o provedor estiver validado, o estado passa a `experimental`, nunca diretamente a `proven_local`.

## Sequência de implementação no Moddin

1. **Inventário declarativo:** introduzir `ai_recipe` fora do `config` plano atual, com schema versionado, evidência e limites por campo.
2. **Preflight real:** detectar arquitetura, API, ReShade/add-on support, UEVR, conflito de rota, provedor de DLSS e modo VR antes de habilitar “Aplicar”.
3. **Rotas separadas:** manter o módulo ReShade existente e criar um módulo UEVR distinto; ambos compartilham diagnóstico, mas não instalação.
4. **Perfis e diffs:** permitir que a IA proponha somente alterações em `sr`, `gaze` ou `nr`, exibindo diff, riscos e confirmação quando necessário.
5. **Sessão de teste:** registrar A/B, métricas, observações e diagnóstico; a compatibilidade do app deve consumir esses dados.
6. **Aprendizado local:** reutilizar `proven_local` somente quando versão, rota e provedor continuam iguais; nunca promover automaticamente para o catálogo global.

## Fontes

[^1]: ClarkCheekyKent. [Release v0.3.4](https://github.com/ClarkCheekyKent/CheekyFoveatedDLSS/releases/tag/v0.3.4). Atualização dos três artefatos e correção de calibração OpenXR.
[^2]: ClarkCheekyKent. [README do Cheeky Foveated DLSS](https://github.com/ClarkCheekyKent/CheekyFoveatedDLSS/blob/master/README.md). Requisitos, rotas de instalação, VR e lista upstream de jogos testados.
[^3]: ReShade. [Página oficial](https://reshade.me/component/content/featured?Itemid=101). Edição com suporte completo a add-ons e sua natureza não assinada.
[^4]: ClarkCheekyKent. [Issue #15 — Crimson Desert](https://github.com/ClarkCheekyKent/CheekyFoveatedDLSS/issues/15), [#14 — Blood of Dawnwalker](https://github.com/ClarkCheekyKent/CheekyFoveatedDLSS/issues/14), [#27 — AFW](https://github.com/ClarkCheekyKent/CheekyFoveatedDLSS/issues/27), [#30 — Banishers](https://github.com/ClarkCheekyKent/CheekyFoveatedDLSS/issues/30), [#10 — IL-2 Korea/Samsung XR](https://github.com/ClarkCheekyKent/CheekyFoveatedDLSS/issues/10). Relatos de falhas e limitações por rota/jogo.
[^5]: ClarkCheekyKent. [USAGE.md](https://github.com/ClarkCheekyKent/CheekyFoveatedDLSS/blob/master/USAGE.md). Defaults, controles, diagnósticos, suporte e limites do DLSS-NR.
[^6]: ClarkCheekyKent. [EYE-CALIBRATION.md](https://github.com/ClarkCheekyKent/CheekyFoveatedDLSS/blob/master/EYE-CALIBRATION.md). Rotas suportadas, camada OpenXR e critérios de calibração.
[^7]: ClarkCheekyKent. [Campos persistidos](https://github.com/ClarkCheekyKent/CheekyFoveatedDLSS/blob/master/uevr/settings_fields.inc) e [validação de settings](https://github.com/ClarkCheekyKent/CheekyFoveatedDLSS/blob/master/src/settings.hpp). Nomes, defaults e faixas permitidas.
[^8]: ClarkCheekyKent. [Auditoria de DLSS-NR](https://github.com/ClarkCheekyKent/CheekyFoveatedDLSS/blob/master/NR-COMPARISON.md). Limites medidos e ressalvas de compatibilidade.
[^9]: CD PROJEKT RED. [DLSS 4 no Cyberpunk 2077](https://support.cdprojektred.com/en/cyberpunk/pc/sp-technical/issue/2774/dlss-4-multi-frame-generation-e-modelo-transformer). Confirma a presença de Super Resolution no jogo.
[^10]: OptiScaler. [Elden Ring (ERSS-FG)](https://github.com/optiscaler/OptiScaler/wiki/Elden-Ring-%28ERSS%E2%80%90FG%29). Exemplo de provedor externo de upscaling/Frame Generation para Elden Ring.
[^11]: Comunidade r/virtualreality. [Tópico de lançamento e testes](https://www.reddit.com/r/virtualreality/comments/1w6hurl/cheekyfoveateddlss_foveated_dlss_reshade_addon/). Relato do autor sobre Cyberpunk e experiências não verificadas de usuários.
