# Conectando uma IA local ao Moddin

O Moddin inclui um botão **IA local** na barra lateral. Esse botão abre um painel que detecta as IAs instaladas no seu PC e registra o Moddin como servidor MCP local — sem você editar JSON, sem `npm install`, sem prompt pra copiar e colar.

> 🛡️ **O Moddin nunca grava nas pastas dos jogos.** A IA escreve apenas YAMLs em `%LOCALAPPDATA%\Moddin\capabilities\`. Você sempre aplica a capability pelo Moddin, com preview e rollback.

---

## Quem usa isso

Você quer conversar com o Cursor, Claude Desktop ou Codex e pedir *"adiciona esse FPS Unlocker em Elden Ring"* — sem abrir o Moddin pra escrever YAML manualmente. Com a IA local conectada:

1. Você fala em linguagem natural.
2. A IA usa as ferramentas MCP (`list_supported_games`, `validate_capability_yaml`, `preview_capability_plan`, `save_capability_yaml`).
3. O YAML validado cai em `%LOCALAPPDATA%\Moddin\capabilities\`.
4. Na próxima vez que abrir o Moddin, a nova capability aparece — você clica **Apply**.

Nenhuma das duas IAs é obrigatória. Se você só quer o fluxo antigo de "Peça ajuda à IA" (colar prompt num navegador), continue usando — esse botão continua no app.

---

## O outro fluxo: "Peça ajuda à IA" (sem MCP)

O botão **Pedir ajuda à IA** (barra lateral e menu do jogo) é o fluxo para quem não conectou nenhuma IA local. Ele agora fecha o ciclo completo dentro do app:

1. O Moddin monta o prompt (descreva o que quer em uma frase).
2. Você cola o prompt no ChatGPT / Claude / Gemini.
3. A IA devolve um YAML — cole de volta no Moddin.
4. O Moddin **valida contra o schema real** (`validate_capability_yaml`), mostra o **plano de instalação** (`preview_capability_plan`) e **salva** (`save_capability_yaml`) em `%LOCALAPPDATA%\Moddin\capabilities\`, com backup do arquivo anterior e confirmação antes de substituir.

Os três commands vivem em `src-tauri/src/capability_authoring.rs` e usam exatamente o mesmo `CapabilitySpec` que o runner executa — o que a IA escreve é verificado como qualquer capability embutida. Depois de salvar, o mod aparece **na hora** como card na tela do jogo (seção "Mods da IA"), e também na lista do painel **Comunidade** com a etiqueta **Local**.

> ⚠️ Salvar não instala. A instalação continua sendo uma decisão sua, feita pelo Moddin com preview e rollback.

### Modo agente: a IA faz por você (sem copiar/colar)

Se você tem o **Codex**, o **Claude Code** ou o **Cursor Agent** instalado, o passo 1 do diálogo mostra um cartão "Sua IA instalada pode fazer isso por você". Um clique em **Fazer com {agent}** faz o Moddin:

1. Executar a CLI em modo headless (`run_ai_agent_prompt` em `src-tauri/src/ai_agent_runner.rs`) com o prompt do fluxo — o agente usa as ferramentas MCP do Moddin (`list_supported_games`, `get_capability_template`, `validate_capability_yaml`, `preview_capability_plan`) para se orientar no catálogo real.
2. Capturar a resposta final, extrair o bloco YAML e **pular direto para a tela de revisão** — você confere o plano dry-run e clica em **Salvar mod**.

Nada é salvo sem essa confirmação. Se o agente não devolver YAML (por exemplo, no modo Diagnóstico), a resposta em texto aparece no diálogo para você ler.

Detalhes de implementação:

- **Codex**: o CLI só lê `$CODEX_HOME/config.toml` (`[mcp_servers.moddin]`) — o setup do painel **IA local** espelha a entry tanto no `mcp.json` quanto no `config.toml`, com backup e escrita atômica.
- A execução usa `--ephemeral`, um diretório de trabalho próprio (`%LOCALAPPDATA%\Moddin\agent-runs\`), timeout de 4 minutos e uma run por vez.
- Sem nenhuma IA instalada, o cartão simplesmente não aparece — o fluxo de copiar/colar continua funcionando.

---

## Passo a passo

### 1. Abra o Moddin Desktop
Qualquer build do Moddin Desktop já vem com o botão **IA local** na barra lateral.

### 2. Clique em **IA local**
O painel abre e dispara `detect_ai_assistants`. O Moddin procura, em paralelo, três IAs:

| IA | Onde o Moddin procura |
|---|---|
| **Cursor** | `%LOCALAPPDATA%\Programs\Cursor\Cursor.exe` ou `where cursor` no PATH |
| **Claude Desktop** | `%APPDATA%\Claude\Claude.exe` (config em `%APPDATA%\Claude\claude_desktop_config.json`) |
| **Codex CLI** | `where codex` no PATH (config em `%USERPROFILE%\.codex\mcp.json`) |

Cada uma vira um card com um status:

- 🟢 **Conectada** — a IA está instalada e o Moddin já tem um `mcpServers.moddin` apontando pro runtime embarcado.
- 🟡 **Instalada** — a IA existe mas o Moddin ainda não registrou; clique em **Conectar**.
- 🔴 **Erro** — a config existe mas aponta pra outro lugar (geralmente outra versão antiga do MCP).
- ⚪ **Não instalada** — a IA não foi encontrada. Instale e clique em **Atualizar status**.

### 3. Clique em **Conectar**
O painel chama `setup_ai_assistant`, que:

1. Lê o arquivo de configuração existente (preserva as outras entries suas, por exemplo o `hindsight`).
2. Faz backup automático (`mcp.json.moddin-bak`).
3. Escreve atomicamente a entry `moddin` apontando pro runtime embarcado do Moddin:

   ```json
   {
     "mcpServers": {
       "hindsight": { "…": "…", "…": "…" },
       "moddin": {
         "command": "C:/Program Files/Moddin Desktop/resources/node.exe",
         "args": ["C:/Program Files/Moddin Desktop/resources/moddin-agent/src/mcp-server.mjs"],
         "env": {
           "MODDIN_LOCAL_CAPABILITIES_DIR": "",
           "MODDIN_PROJECT_ROOT": "C:/Program Files/Moddin Desktop"
         }
       }
     }
   }
   ```
4. Devolve o card atualizado, agora **🟢 Conectada**.

### 4. Reinicie a IA
A maioria dos clientes MCP só lê a config no boot. **Feche e abra de novo** o Cursor / Claude Desktop / Codex. Na próxima inicialização, a IA carrega o runtime Moddin via stdio e passa a enxergar as ferramentas `moddin.*`.

### 5. Teste no chat da IA
Abra qualquer chat e peça:

```
Você tem o MCP "moddin" disponível? Chame list_supported_games e me devolva a lista de jogos.
```

Se a IA listar **Cyberpunk 2077, Elden Ring, STALKER 2, Dawnwalker, Dead Island 2, DOOM 2016**, a conexão funciona.

### 6. Crie uma capability
Agora peça em linguagem natural:

```
Quero instalar um mod DLSS Swap no Cyberpunk 2077. Baixe o release mais recente
de <URL_do_release_no_GitHub>, me passe o caminho do ZIP, e monte a capability
usando o padrão extract-zip. Valida, faz preview, e só salva depois que eu confirmar.
```

A IA vai:
- Chamar `get_game_info({gameId:"cyberpunk-2077"})`.
- Chamar `get_capability_template({pattern:"extract-zip"})`.
- Preencher campos e te pedir o SHA-256.
- Chamar `validate_capability_yaml` → `ok: true`.
- Chamar `preview_capability_plan` → diff de arquivos/registry.
- Pedir confirmação explícita.
- Chamar `save_capability_yaml` → escreve em `%LOCALAPPDATA%\Moddin\capabilities\dlss-swap.yaml`.

Abra o Moddin, clique em **Rescan stores**, escolha Cyberpunk 2077 → a capability está lá → clique em **Apply**.

### 7. Desconectar
No painel **IA local**, clique em **Desconectar** no card da IA. O Moddin remove só a entry `moddin` da config, restaurando o backup. As outras entries suas ficam intactas.

---

## O que vai dentro do instalador do Moddin

Quando você roda `npm run tauri build` ou o workflow do CI, o Tauri empacota `moddin-runtime/` como recurso:

```
moddin-runtime/
  node.exe              ← Node.js 20 portable (~68 MB)
  moddin-agent/         ← prod-only (~70 MB)
    package.json
    src/mcp-server.mjs
    scripts/smoke-test.mjs
    templates/*.yaml
    schema/capability.schema.json
    node_modules/       ← deps de produção
```

O Moddin final (MSI) instala isso em `<install>/resources/` e o Tauri command resolve `app.path().resource_dir()` automaticamente. O usuário não precisa ter Node no sistema.

---

## Troubleshooting

### "Bundled MCP server not found"

O painel mostra esse erro quando o Tauri não consegue localizar `node.exe` ou `moddin-agent/src/mcp-server.mjs`. Causas comuns:

- **Build em desenvolvimento:** o `moddin-runtime/` ainda não foi gerado. Rode `powershell -ExecutionPolicy Bypass -File moddin-runtime/bundle-runtime.ps1`.
- **Instalação parcial:** o instalador MSI não terminou. Reinstale o Moddin.
- **Antivírus:** alguns produtos Windows Defender/Defender for Endpoint apagam o `node.exe` por heurística. Adicione a pasta de instalação do Moddin às exceções.

### A IA não enxerga o MCP depois de Conectar

- **Reiniciou a IA?** Feche e abra o Cursor/Claude/Codex depois de mudar a config.
- **Config errada:** abra manualmente o arquivo (`%APPDATA%\Claude\claude_desktop_config.json` ou `~/.cursor/mcp.json`) e veja se `mcpServers.moddin.command` e `args` batem com o caminho de instalação do Moddin.
- **Codex específico:** o Codex CLI ainda não tem um caminho estável para a config em todas as versões. Se você instalou via `npm i -g @openai/codex`, a config geralmente fica em `%USERPROFILE%\.codex\mcp.json`.

### "ConfigError: Moddin MCP entry points at a different binary"

A config existe mas o `command` aponta pra um caminho antigo (por exemplo, de uma versão anterior do Moddin). Clique em **Desconectar** e depois em **Conectar** — o painel regrava a entry com o caminho atual.

### A IA cria uma capability e ela não aparece no Moddin

- O Moddin só lê `%LOCALAPPDATA%\Moddin\capabilities\` ao iniciar (ou quando você clica em **Rescan stores**).
- A IA precisa ter chamado `save_capability_yaml` com sucesso e `result.ok === true`. Peça pra IA mostrar o JSON-RPC reply.
- Confira se a env `MODDIN_LOCAL_CAPABILITIES_DIR` no `mcp.json` é `""` (vazio, não `null`). O Moddin interpreta vazio como "use o caminho padrão".

### Quero dar Desconectar e o Moddin deu erro

Abra o arquivo de config à mão (o painel mostra o caminho no card) e remova a chave `moddin` manualmente. O Moddin não toca em nenhuma outra chave.

---

## Para desenvolvedores / contribuidores

- O runtime fica em `moddin-runtime/`, gerado por `moddin-runtime/bundle-runtime.ps1`. Não é commitado (ver `.gitignore`).
- O Tauri command surface está em `src-tauri/src/ai_assistant_setup.rs`. São 3 commands: `detect_ai_assistants`, `setup_ai_assistant`, `remove_ai_assistant`. Cada um aceita `resource_dir: String` para localizar o runtime embarcado.
- O frontend está em `src/features/local-ai/`. É totalmente desacoplado do feature `src/features/ai-assistant/` (que é o fluxo de "Peça ajuda à IA" para web AIs).
- Os testes Rust (`cargo test --lib ai_assistant_setup`) cobrem: round-trip setup/remove, idempotência, layout inválido, JSON malformado, detecção sem IA instalada.

Para adicionar uma nova IA ao painel (Warp, Continue.dev, Cline, etc.):

1. Adicione um novo `AgentId` em `src-tauri/src/ai_assistant_setup.rs`.
2. Adicione o `LAYOUTS` com o caminho da config e o detector de binário.
3. Adicione o `display_name`.
4. Adicione o `AgentId` em `src/features/local-ai/types.ts` e o card correspondente no painel se a UX pedir um layout diferente.
