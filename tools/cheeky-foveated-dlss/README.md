# Cheeky Foveated DLSS — Moddin verifier/installer

Scanner + instalador conservador para **STALKER 2 UEVR**, **Dead Island 2 UEVR**, **Cyberpunk 2077 VR Port** e **Elden Ring ERVR**.

## Uso

- `VERIFICAR.cmd`: só detecta jogos, pipeline VR, DLSS, UEVR/ReShade e conflitos. Não altera nada.
- `INSTALAR.cmd`: instala a release estável mais recente do Cheeky nos alvos que passaram nas regras. Inclui candidatos experimentais somente quando todos os pré-requisitos detectáveis estão presentes.
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
DESINSTALAR.cmd
scripts/
  CheekyFoveatedDLSS.ps1
backups/   # criado em runtime
logs/      # criado em runtime
state/     # criado em runtime
.cache/    # criado em runtime
```

Pastas de runtime ficam fora do Git.
