# Cheeky Foveated DLSS — Moddin verifier/installer

Scanner + instalador conservador para **STALKER 2 UEVR**, **Dead Island 2 UEVR**, **Cyberpunk 2077 VR Port** e **Elden Ring ERVR**.

## Uso

- `VERIFICAR.cmd`: só detecta jogos, pipeline VR, DLSS, UEVR/ReShade e conflitos. Não altera nada.
- `INSTALAR.cmd`: modo conservador; instala somente alvos `READY`.
- `INSTALAR-EXPERIMENTAIS.cmd`: também permite alvos `EXPERIMENTAL_READY`, mas apenas quando todos os pré-requisitos detectáveis passaram.
- `DESINSTALAR.cmd`: remove apenas os arquivos instalados pelo script e restaura backups quando existiam arquivos anteriores.

O script baixa os assets diretamente da release oficial `ClarkCheekyKent/CheekyFoveatedDLSS`, valida SHA-256 quando o GitHub publica `digest` no asset, mantém logs e backups locais e executa o `CheekyOpenXRSetup.exe` da mesma release quando há instalação.

## Matriz usada pelo scanner

| Jogo | Integração | Regra |
|---|---|---|
| STALKER 2 | UEVR plugin | `READY` quando UEVR config + DLSS nativo estão presentes e ReShade não conflita. |
| Dead Island 2 | UEVR plugin | `EXPERIMENTAL_READY` somente com UEVR + OptiScaler + `nvngx_dlss.dll`. O jogo base oferece FSR2, não DLSS. |
| Cyberpunk 2077 | ReShade add-on | `EXPERIMENTAL_READY` com VR Port + DLSS nativo + ReShade full add-on host. Proxies VR antigos que ocupam `dxgi.dll` são bloqueados. |
| Elden Ring | ReShade add-on | `EXPERIMENTAL_READY` com Ilya ERVR + ReShade + ERSS-FG + `ERSS2\bin\nvngx_dlss.dll`. O jogo base não possui upscaler DLSS. |

`READY` significa que o jogo caiu no caminho de integração oficialmente suportado pelo Cheeky e não há conflito detectável. Isso não equivale a dizer que aquela versão específica do jogo foi validada pelo autor.

`EXPERIMENTAL_READY` significa **pipeline tecnicamente coerente, mas combinação ainda não listada como oficialmente testada pelo Cheeky**.

### O que o scanner não consegue provar sozinho

- Que o OptiScaler está efetivamente selecionando **DLSS SR em runtime**; ele consegue confirmar os arquivos necessários, não o estado do overlay enquanto o jogo roda.
- A versão exata da **Plugin API do UEVR** em todas as formas de instalação. O Cheeky exige API 2.39.0 ou 2.x compatível mais nova.
- Compatibilidade de uma atualização futura do jogo/VR mod. O scanner evita conflitos de arquivos conhecidos, mas não substitui o teste visual/frametime dentro do headset.

Por isso rode `VERIFICAR.cmd` primeiro e trate `EXPERIMENTAL_READY` literalmente como experimental.

## Quest 3

Comece com os defaults do Cheeky e use:

- `Foveation center = Fixed`
- `Automatic stereo alignment = On`
- `Automatic eye calibration = On`
- DLSS-NR desligado inicialmente

Use a borda vermelha de calibração e confira os dois olhos antes de reduzir mais a região foveada.

## Elden Ring

O suporte é voltado ao **Ilya ERVR** atual (OpenXR/ReShade), não ao R.E.A.L. VR antigo. ERVR exige jogo offline com Easy Anti-Cheat desabilitado. Este script **não altera ou contorna anti-cheat**.

Se ERSS-FG e OptiScaler forem usados juntos com ERVR, mantenha o `dxgi.dll` do ReShade/ERVR e prefira carregar OptiScaler por outro proxy suportado (por exemplo `version.dll` quando aplicável), para não sobrescrever o host ReShade.

## Estrutura local

```text
VERIFICAR.cmd
INSTALAR.cmd
INSTALAR-EXPERIMENTAIS.cmd
DESINSTALAR.cmd
scripts/
  CheekyFoveatedDLSS.ps1
backups/   # criado em runtime
logs/      # criado em runtime
state/     # criado em runtime
.cache/    # criado em runtime
```

Pastas de runtime ficam fora do Git.
