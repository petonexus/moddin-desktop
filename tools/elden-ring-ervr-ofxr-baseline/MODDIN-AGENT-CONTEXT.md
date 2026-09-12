# Moddin — Elden Ring VR: baseline ERVR + VDXR + OFXR

**Status do documento:** baseline funcional definido em 12/09/2026  
**Objetivo:** dar ao agente do Moddin um ponto de partida conhecido, evitando repetir os experimentos que deixaram o pipeline mais complexo e instável.

---

## 1. Estado que deve ser considerado o baseline

O estado que efetivamente chegou a funcionar no PC do Sr. Marco foi:

```text
ELDEN RING
  ↓
ReShade 6.8 add-on build
  ↓
ERVR 0.4.0
  ↓
OpenXR / VDXR
  ↓
OFXR Bridge
  ↓
Quest 3
```

Configuração ERVR usada como baseline:

```ini
[VR]
StereoMode=full
GameRes=auto
GameResScale=1.0
RenderScale=1.0

[Patches]
FpsTarget=60

[FirstPerson]
CameraBob=0

[HUD]
Mode=quad
Lock=body

[General]
LogLevel=debug
```

**Importante:** OFXR faz parte do baseline funcional, mas este pacote **não atualiza, reinstala nem reconfigura OFXR automaticamente**. Deve-se preservar a versão/configuração que já funcionou e apenas confirmar que a layer está armada antes de iniciar o jogo.

A versão upstream do OFXR em 12/09/2026 é `v0.2.0 / V068`, ainda classificada como pre-release experimental. Se for atualizar a versão instalada, isso deve ser tratado como **um experimento separado**, nunca junto de outra mudança.

---

## 2. Hardware/runtime do baseline testado

Contexto do usuário:

- GPU: NVIDIA RTX 4080 Super
- Headset: Meta Quest 3
- Runtime preferido no teste funcional: Virtual Desktop / VDXR
- ERVR: 0.4.0
- Elden Ring observado: exe `2.7.1.0` / patch 1.17.1
- ReShade: build 6.8 com full add-on support fornecido/requerido pelo ERVR

O ERVR 0.4.0 oficialmente possui mapas completos para `eldenring.exe 2.6.2.0` e `2.7.1.0`.

---

## 3. Layout mínimo esperado em `ELDEN RING\Game`

Preservar:

```text
ELDEN RING\Game\
├─ eldenring.exe
├─ dxgi.dll
├─ ReShade.ini
├─ dinput8.dll
└─ ERVR\
   ├─ ERVR.dll
   ├─ ERVR.ini
   └─ ...
```

Funções:

- `dxgi.dll` = ReShade 6.8 add-on build usado pelo ERVR.
- `dinput8.dll` = loader padrão do ERVR, salvo quando outro mod loader é usado.
- `ERVR\ERVR.dll` = mod VR.
- `ERVR\ERVR.ini` = configuração.

Não substituir `dxgi.dll` por OptiScaler, R.E.A.L. VR ou outro proxy enquanto este baseline estiver ativo.

---

## 4. Componentes explicitamente FORA do baseline

Não instalar/ativar automaticamente:

```text
CheekyFoveatedDLSS.addon64
Cheeky OpenXR implicit layer
ERSS-FG.dll
ERSSReShadeStub.addon
ERSS2\
OptiScaler
DLSS / nvngx_dlss*.dll
Streamline sl.*.dll
R.E.A.L. VR
```

### Motivo

O Elden Ring vanilla **não possui DLSS**. Para dar DLSS ao jogo foi necessário experimentar ERSS-FG.

A cadeia experimental ficou aproximadamente:

```text
ERVR
+ ReShade
+ ERSS-FG
+ ERSSReShadeStub
+ DLSS
+ OFXR
```

Isso introduziu mais loaders/hooks e o ERSS chegou a informar falha de comunicação com o add-on do ReShade.

Não houve benefício comprovado suficiente para justificar a complexidade. Portanto, **DLSS/ERSS/Cheeky/OptiScaler não pertencem ao baseline**.

---

## 5. Experimentos realizados e conclusões

### `GameResScale=0.65`

Resultado:

- mais estável em teste;
- imagem considerada muito embaçada;
- inadequado para uso normal no Quest 3.

Conclusão:

> bom apenas para diagnóstico de performance.

### `GameResScale=0.85`

Criado como preset intermediário. Não virou o estado final escolhido.

### `GameResScale=1.0 + OFXR`

Resultado:

- abriu e funcionou;
- qualidade visual claramente melhor;
- escolhido como baseline funcional atual.

Conclusão:

> baseline a preservar.

### Cheeky Foveated DLSS

Problema estrutural:

- Cheeky precisa interceptar DLSS Super Resolution real.
- Elden Ring não tem DLSS nativo.
- Isso obrigou adicionar ERSS-FG/ERSS2, aumentando muito o número de hooks.

Conclusão:

> fora do baseline.

### ERSS-FG / DLSS

Foram encontrados/experimentados:

```text
D3D12.dll
ERSS-FG.dll
ERSS2\
ERSSReShadeStub.addon
```

O ERSS chegou a mostrar erro de conexão com o add-on ReShade.

Conclusão:

> remover/desativar no baseline. Retomar apenas como experimento isolado no futuro.

### OptiScaler

Não necessário para o estado funcional atual.

Conclusão:

> não instalar no baseline.

---

## 6. Bugs/sintomas observados no ERVR

Mesmo no pipeline básico, o ERVR 0.4.0 ainda é um mod jovem.

Observado pelo usuário:

- em alguns launches houve travamento/crash;
- em outro launch funcionou;
- sensação de câmera "se arrastando";
- após morrer/respawnar, a câmera pareceu melhorar;
- uma notificação de item coletado ficou presa na tela;
- menus/transições apresentaram comportamento estranho.

Notas:

- menus, cutscenes, loading e transições aparecerem em **cinema panel** é comportamento intencional do ERVR;
- HUD preso, câmera mudando de estado e crashes não são desejáveis.

Atalhos úteis do HUD do ERVR:

```text
NumLock ON
Numpad .  -> recentraliza HUD
Numpad 7  -> toggle HUD
Numpad 8  -> alterna UI-draw split
```

---

## 7. Crash observado

Houve múltiplos crashes com assinatura semelhante no ReShade:

```text
exception: 0xc0000005
module: dxgi.dll
offset observado: +0x1249a2
```

Isso aconteceu mais de uma vez, inclusive antes de alguns experimentos posteriores.

Portanto:

> não atribuir automaticamente qualquer crash ao OFXR ou DLSS sem logs.

ERVR deve permanecer com:

```ini
[General]
LogLevel=debug
```

Para crash/freeze, coletar:

- `Game\ERVR\ERVR.log`
- `Game\ReShade.log`
- Event Viewer / Application Error / Application Hang
- OFXR Flight Recorder se OFXR estiver ativo

---

## 8. OFXR

OFXR Bridge é uma OpenXR API layer de frame generation por optical flow.

No baseline:

```text
ERVR -> OpenXR/VDXR -> OFXR -> headset
```

OFXR deve ser armado **antes de iniciar o jogo**.

Não há necessidade de instalar Frame Generation do ERSS junto.

Evitar:

```text
ERSS/DLSS Frame Generation
+
OFXR Frame Generation
```

No upstream atual, a recomendação inicial para NVIDIA é:

```text
Backend: NVIDIA Medium
OFA scale: 50%
```

Mas se o perfil atual do usuário já estiver funcionando, **preservá-lo em vez de mudar duas variáveis ao mesmo tempo**.

---

## 9. Regras para automação no Moddin

Ao oferecer o preset `Elden Ring — ERVR + OFXR Baseline`:

### Pode automatizar

- detectar pasta Steam;
- conferir versão do exe;
- conferir ERVR;
- conferir ReShade add-on;
- editar `ERVR.ini`;
- desativar/quarentenar extras experimentais;
- desativar layer global do Cheeky;
- conferir runtime OpenXR;
- conferir se OFXR está registrado/armado;
- gerar diagnóstico;
- criar backup/restauração.

### Não automatizar nesse preset

- instalar ERSS-FG;
- instalar DLSS;
- instalar Cheeky;
- instalar OptiScaler;
- trocar `dxgi.dll`;
- atualizar OFXR sem consentimento explícito;
- empilhar dois frame generators;
- tocar no anti-cheat.

### Segurança

- ERVR é **offline only**.
- Easy Anti-Cheat precisa estar desativado pelo método offline já configurado pelo usuário.
- O baseline não deve criar mecanismos de bypass de anti-cheat.

---

## 10. Comportamento do script deste pacote

`01-APLICAR-BASELINE.cmd`:

1. localiza `ELDEN RING\Game`;
2. exige os arquivos básicos do ERVR;
3. cria snapshot do `ERVR.ini`;
4. move extras experimentais para:
   `ELDEN RING\_MODDIN_BACKUPS\ERVR-OFXR-Baseline\snapshot-...\quarantine\`
5. nunca apaga os extras;
6. preserva `dxgi.dll`, `dinput8.dll`, `ReShade.ini` e `ERVR\`;
7. aplica o INI do baseline;
8. desativa a layer global do Cheeky OpenXR;
9. **não altera a layer OFXR**;
10. gera `BASELINE-REPORT.txt`.

`00-VERIFICAR-BASELINE.cmd`:

- mostra arquivos básicos;
- mostra config;
- mostra OpenXR runtime;
- mostra OFXR;
- mostra Cheeky;
- mostra extras ainda ativos;
- grava relatório.

`02-RESTAURAR-ESTADO-ANTERIOR.cmd`:

- restaura o `ERVR.ini` anterior;
- devolve arquivos/pastas da quarentena;
- restaura valores anteriores da layer Cheeky OpenXR.

---

## 11. Procedimento de teste recomendado

Depois de aplicar o baseline:

1. abrir Virtual Desktop;
2. confirmar VDXR como OpenXR runtime;
3. abrir `OFXRBridgeTray.exe`;
4. `Arm bridge`;
5. iniciar Elden Ring pelo método offline/EAC-disabled;
6. título/menu no cinema panel é esperado;
7. carregar save;
8. verificar full stereo;
9. jogar uma área conhecida por alguns minutos;
10. morrer/respawnar, abrir menu e coletar item para testar transições/HUD.

Critérios mínimos de aceitação:

```text
[ ] abre repetidamente
[ ] não crasha no loading inicial
[ ] full stereo correto
[ ] head tracking sem arrasto excessivo
[ ] HUD não fica preso
[ ] menus retornam ao gameplay corretamente
[ ] OFXR realmente entra em geração
```

---

## 12. Fontes atuais

ERVR 0.4.0:
- Nexus Mods — Ilya's Elden Ring VR
- https://www.nexusmods.com/eldenring/mods/10711

OFXR Bridge:
- GitHub — tig3rmast3r/OFXR-Bridge
- https://github.com/tig3rmast3r/OFXR-Bridge

Estado upstream consultado em 12/09/2026:
- ERVR: 0.4.0
- OFXR Bridge: pre-release v0.2.0 / internal build V068

---

## 13. Decisão atual

**Não continuar adicionando otimizações ao baseline.**

Primeiro objetivo:

> fazer `ERVR + VDXR + OFXR + GameResScale 1.0` permanecer reproduzível e estável.

Somente depois que isso estiver sólido, qualquer experimento futuro deve ser A/B e adicionar **uma única variável por vez**.

Ordem futura aceitável:

```text
baseline
↓
mudar apenas OFXR
↓
testar
↓
voltar baseline
↓
testar apenas DLSS/ERSS, se ainda fizer sentido
```

Não voltar diretamente para uma cadeia com ERSS + DLSS + Cheeky + OFXR + OptiScaler.
