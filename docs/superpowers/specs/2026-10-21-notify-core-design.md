# notify: núcleo (parte 1), design

- **Data:** 2026-10-21
- **Status:** aprovado em conversa, aguardando revisão deste documento
- **Crate:** `notify-rs` (em `jr-rs/notify-rs`)

## 1. Contexto e objetivo

`notify` é uma biblioteca genérica de interface gráfica no estilo Flutter
(widgets descrevem a interface, elements são as instâncias vivas na árvore).
Outras bibliotecas a usam como base. A primeira prevista é `opengl-notify-rs`,
um backend que desenha com OpenGL.

O objetivo é servir de base para **aplicações reais**: robustez, comportamento
previsível e uma API pública que outras bibliotecas possam estender.

### Fronteira entre `notify` e backends

O `notify` cuida de widgets, elements, layout, hit testing e despacho de
eventos, e pinta numa lista abstrata de comandos de desenho. O backend cria a
janela, entrega eventos ao `notify` e rasteriza os comandos.

### Divisão em partes

O framework é dividido em partes, cada uma com seu próprio ciclo de spec,
plano e implementação. **Este documento cobre só a parte 1.**

| # | Parte | Conteúdo | Depende de |
|---|---|---|---|
| 1 | **Núcleo** | widgets, elements, reconciliação, keys, reconstrução sob demanda, dados compartilhados (`provide`/`get`), integração com `reactive-rs` | — |
| 2 | Render e layout | objetos de render, restrições, protocolo de layout, `Row`/`Column`/`Padding`/`Center`/`Stack` | 1 |
| 3 | Pintura | lista de comandos de desenho, canvas, recorte e transformações | 2 |
| 4 | Texto | fontes, shaping, quebra de linha e medição (provavelmente com `cosmic-text`) | 2, 3 |
| 5 | Backend e frames | trait de backend, loop de frames, backend headless para testes | 1–3 |
| 6 | Entrada | eventos de ponteiro e teclado, hit testing, gestos, foco | 2, 5 |
| 7 | `opengl-notify-rs` | primeiro backend real, em projeto separado | 5 |

## 2. Escopo da parte 1

**Dentro:**
- os traits `Widget`, `Element` e `BuilderWidget`;
- `ElementCore`, `Key`, `BuildContext` e `App`;
- a reconciliação de filhos, inclusive listas com key;
- a reconstrução sob demanda com flags propagadas, com avisos vindos de
  qualquer thread;
- os dados compartilhados pela árvore via `BuildContext::provide`/`get`;
- a mudança no `reactive-rs` descrita na seção 5;
- o workspace Cargo em `jr-rs/`.

**Fora:**
- qualquer widget visual (`Text`, `Row`, `Button`...);
- render, layout, pintura, texto, entrada e backend (partes 2 a 7);
- `GlobalKey` (mover uma subárvore mantendo o estado);
- widgets assíncronos (`FutureBuilder`);
- widget de erro no lugar de uma subárvore que entrou em pânico.

## 3. Decisões tomadas

| Decisão | Escolha | Alternativas descartadas e por quê |
|---|---|---|
| Forma da árvore | **Cada element é dono do seu widget e dos seus filhos** (`Vec<Box<dyn Element>>`). Não há ponteiro para o pai. | Arena com ids (`slotmap`) e um `BuildOwner`: ids em todo lugar e elements retirados da arena durante as chamadas, o que tornaria layout, pintura e hit testing (algoritmos recursivos) pouco naturais em Rust. `Rc<RefCell>` com `Weak` para o pai: pânicos de empréstimo em tempo de execução. |
| Encontrar quem reconstruir | **Flags atômicas propagadas para cima** (`dirty`, `descendant_dirty`). O `update` só desce por galhos marcados. | Lista de sujos: exige endereçar elements, ou seja, voltar à arena. Varredura da árvore inteira: custo proporcional ao tamanho da árvore a cada atualização. O mesmo padrão de "marcar e propagar" vai servir para `needs_layout` e `needs_paint` nas partes 2 e 3. |
| Partes comuns de um element | **`ElementCore`**, embutido em cada element e gerenciado pelo framework | Cada implementação guardar e atualizar widget, filhos e flags: código repetido, e um esquecimento quebraria a reconciliação sem erro de compilação. |
| Estado | **Sem estado local.** Widgets guardam dados e um `Notifier` em campos (via `Arc`) criados fora da árvore. O element assina o notifier e reconstrói quando ele avisa. | `StatefulWidget` + `State` com `set_state`: mais conceitos. Estado criado por `create_state` e guardado pelo element: recusado em favor deste modelo. |
| Dados compartilhados | `ctx.provide(valor)` e `ctx.get::<T>()`, com um escopo que desce pela árvore durante o `update` | Um widget `InheritedWidget` ou `Inherited<T>`. Subir por ponteiros de pai: não há pai. |
| Tipo comum dos widgets | `Rc<dyn Widget>` | Um `AnyWidget` próprio com trait marcador. |
| Extensibilidade | `Widget::create_element` e `Element` **públicos**, para outras bibliotecas criarem elements próprios | Tipos de element internos: impediriam `opengl-notify-rs` e terceiros de criar widgets com elements próprios. |
| Ciclo de vida de um element | Nasce em `create_element` e morre no `Drop`, com os filhos sempre destruídos antes do pai. Não há `mount` nem `unmount`. | Ganchos `mount`/`update`/`unmount`: nada na parte 1 precisa deles. Se uma parte futura precisar, entram como métodos com implementação padrão. |
| Dependências externas | Crates consolidadas são permitidas, com justificativa. A parte 1 não usa nenhuma além do `reactive-rs`. | — |

## 4. Estrutura do repositório

`jr-rs/` passa a ser um workspace Cargo:

```toml
# jr-rs/Cargo.toml
[workspace]
members = ["reactive-rs", "notify-rs"]
resolver = "3"
```

- **`.gitignore` da raiz:** `/target` e `.idea/`. Com o workspace, `target/`
  e `Cargo.lock` ficam na raiz. Os `Cargo.lock` dos membros deixam de ser
  usados e são removidos.
- **`notify-rs/Cargo.toml`:**

  ```toml
  [package]
  name = "notify-rs"
  version = "0.1.0"
  edition = "2024"
  license = "MIT"

  [dependencies]
  reactive-rs = { path = "../reactive-rs" }
  ```

- **Módulos de `notify-rs/src/`:**

  | Arquivo | Conteúdo |
  |---|---|
  | `lib.rs` | declara os módulos e reexporta a API pública |
  | `widget.rs` | `Widget`, `BuilderWidget`, `impl<B: BuilderWidget> Widget for B` |
  | `builder_element.rs` | `BuilderElement<B>`, interno |
  | `element.rs` | trait `Element` e `ElementCore` |
  | `flags.rs` | flags atômicas e sinal de "precisa atualizar", internos |
  | `key.rs` | `Key` |
  | `context.rs` | `BuildContext` e o escopo de `provide`/`get` |
  | `reconcile.rs` | atualização de um filho e reconciliação de listas |
  | `app.rs` | `App`: raiz, passada de `update` e `debug_tree` |
  | `test_support.rs` | widgets e elements de teste (só `cfg(test)`) |

- O esqueleto atual de `notify-rs/src/lib.rs` é substituído por completo.

## 5. Mudança no `reactive-rs`

`ValueListenable<T>` ganha um método para ler o valor atual:

```rust
pub trait ValueListenable<T>: Send + Sync {
    fn get(&self) -> T;
    fn add_listener(&self, listener: ValueListener<T>) -> usize;
    fn remove_listener(&self, id: usize);
    // subscribe: sem mudança
}

impl<T: Clone + Send> ValueListenable<T> for ValueNotifier<T> {
    fn get(&self) -> T { /* mesmo valor do ValueNotifier::get inerente */ }
    // ...
}
```

O `impl` passa de `T: Send` para `T: Clone + Send`. Nada útil se perde,
porque `set` e `update` já exigem `Clone`. O README do `reactive-rs` menciona
o novo método, e um teste cobre `get` chamado através de
`&dyn ValueListenable<T>`.

## 6. API pública do `notify-rs`

### 6.1 `Widget`

```rust
pub trait Widget: Any {
    fn key(&self) -> Option<Key> { None }
    fn notifier(&self) -> Option<&Arc<Notifier>> { None }
    fn create_element(self: Rc<Self>) -> Box<dyn Element>;

    #[doc(hidden)]
    fn type_name(&self) -> &'static str { std::any::type_name::<Self>() }
}
```

- Widgets são configurações imutáveis, passadas como `Rc<dyn Widget>`.
- `Rc` (e não `Box`) permite que pai e element compartilhem o mesmo widget,
  torna barato clonar, e permite a regra "mesma instância → pula a subárvore"
  com `Rc::ptr_eq`. `Arc` não é necessário, porque a árvore vive numa thread
  só.
- `Widget: Any` dá ao framework o `TypeId` do tipo concreto e permite
  converter `Rc<dyn Widget>` de volta para `Rc<W>`.
- **`notifier`:** se devolver `Some`, o framework assina esse notifier
  enquanto o element existir. Um aviso marca o element como sujo.
- **`create_element`** recebe o próprio widget em `Rc` e devolve o element,
  já com o `ElementCore` criado a partir desse widget.
- `type_name` serve para o `debug_tree` e não deve ser sobrescrito.

### 6.2 `Element` e `ElementCore`

```rust
pub trait Element {
    fn core(&self) -> &ElementCore;
    fn core_mut(&mut self) -> &mut ElementCore;
    fn build(&mut self, ctx: &mut BuildContext<'_>) -> Vec<Rc<dyn Widget>>;

    // Prontos, vêm do core:
    fn widget(&self) -> &Rc<dyn Widget> { self.core().widget() }
    fn children(&self) -> &[Box<dyn Element>] { self.core().children() }
    fn mark_dirty(&self) { self.core().mark_dirty() }
}

pub struct ElementCore { /* campos privados, seção 7.1 */ }

impl ElementCore {
    pub fn new(widget: Rc<dyn Widget>) -> Self;
    pub fn widget(&self) -> &Rc<dyn Widget>;
    pub fn widget_as<W: Widget>(&self) -> Rc<W>;
    pub fn children(&self) -> &[Box<dyn Element>];
    pub fn mark_dirty(&self);
    pub fn dirty_listener(&self) -> Listener;
}
```

Contrato de quem implementa `Element`:
- Embutir um `ElementCore`, criado com `ElementCore::new(widget)` dentro de
  `Widget::create_element`, e devolvê-lo em `core` e `core_mut`.
- **`build`** devolve os widgets filhos, em ordem. É chamado:
  - logo depois de o element entrar na árvore;
  - sempre que o pai entrega um widget novo compatível (seção 7.2);
  - quando o element foi marcado como sujo;
  - quando uma dependência de `get` mudou (seção 8).

  O framework reconcilia a lista devolvida. Quem implementa um element não
  reconcilia nada.
- O element não altera os filhos diretamente. Só o framework os altera.
- Recursos próprios do element são liberados no `Drop` dele. O framework
  garante que, ao remover um element, os filhos são destruídos antes dele.
- **Campos do `ElementCore`:** são privados e só o framework os altera: troca
  o widget, reconcilia os filhos, gerencia a assinatura do notifier e as
  flags.
- **`widget_as::<W>()`** devolve o widget já com o tipo concreto. Entra em
  pânico se o tipo não for `W`, o que nunca acontece para o próprio tipo de
  um element, porque o framework só reaproveita um element quando o tipo do
  widget é o mesmo.
- **`mark_dirty`** pode ser chamado de qualquer thread, através de
  `dirty_listener`.
- **`dirty_listener()`** devolve um `Listener` do `reactive-rs`
  (`Arc<dyn Fn() + Send + Sync>`) que chama `mark_dirty`. Um element próprio
  usa isso para assinar outros notifiers além do `Widget::notifier`, e deve
  cancelar essas assinaturas no próprio `Drop`.
- **Evolução:** a parte 2 vai acrescentar ganchos de render e layout como
  métodos com implementação padrão, e acesso mutável aos filhos para
  algoritmos recursivos. Isso não quebra quem já implementa `Element`.

### 6.3 `BuilderWidget`

```rust
pub trait BuilderWidget: Any {
    fn key(&self) -> Option<Key> { None }
    fn notifier(&self) -> Option<&Arc<Notifier>> { None }
    fn build(&self, ctx: &mut BuildContext<'_>) -> Rc<dyn Widget>;

    /// Roda `change` e depois chama `notify_listeners()` no notifier.
    /// Entra em pânico se o widget não tiver notifier.
    fn set(&self, change: impl FnOnce()) where Self: Sized { /* ... */ }
}

impl<B: BuilderWidget> Widget for B {
    fn key(&self) -> Option<Key> { BuilderWidget::key(self) }
    fn notifier(&self) -> Option<&Arc<Notifier>> { BuilderWidget::notifier(self) }
    fn create_element(self: Rc<Self>) -> Box<dyn Element> {
        Box::new(BuilderElement::<B>::new(self))
    }
}
```

- `BuilderWidget` é o trait que quem usa o framework implementa no dia a dia.
  Sem notifier, é um widget composto sem estado. Com notifier, ele é
  reconstruído sempre que o notifier avisa.
- `BuilderElement<B>` (interno) contém só um `ElementCore`. O `build` dele
  devolve `vec![self.core.widget_as::<B>().build(ctx)]`.
- `Notifier` é o `reactive_rs::Notifier`.
- O compilador confirmou que uma crate de fora pode implementar `Widget`
  direto para um tipo próprio, apesar do impl genérico, desde que o tipo não
  implemente `BuilderWidget`.

**Onde mora o dado.** Dados e notifier ficam em campos do widget, via `Arc`,
e precisam ser criados por quem vive mais que o widget: o topo da aplicação
ou um modelo. Um pai que cria `Arc::new(Notifier::new())` dentro do próprio
`build` recria o notifier a cada reconstrução. A documentação de
`BuilderWidget` explica isso.

Exemplo (`Button` é da parte 6, aqui só para ilustrar):

```rust
#[derive(Clone)]
struct Counter {
    notifier: Arc<Notifier>,
    count: Arc<AtomicI32>,
}

impl BuilderWidget for Counter {
    fn notifier(&self) -> Option<&Arc<Notifier>> { Some(&self.notifier) }

    fn build(&self, _ctx: &mut BuildContext<'_>) -> Rc<dyn Widget> {
        let this = self.clone();
        Rc::new(Button::new(format!("{}", self.count.load(Relaxed)), move || {
            this.set(|| { this.count.fetch_add(1, Relaxed); })
        }))
    }
}
```

### 6.4 `Key`

```rust
#[derive(Clone)]
pub struct Key(/* Rc<dyn KeyValue> */);

impl Key {
    pub fn new<V: Hash + Eq + Debug + 'static>(value: V) -> Key;
}
// PartialEq, Eq, Hash, Debug
// From<&str> e From<String> (guardam String), From<i32>, From<i64>, From<u32>, From<u64>, From<usize>
```

- Duas keys são iguais quando o tipo do valor e o valor são iguais. Por
  exemplo, `Key::new(1u32) != Key::new(1i64)`, e
  `Key::from("a") == Key::new(String::from("a"))`.
- O `Debug` mostra o `Debug` do valor, como `"a"` ou `1`.

### 6.5 `BuildContext`

```rust
pub struct BuildContext<'a> { /* escopo + buffers do element em construção */ }

impl BuildContext<'_> {
    pub fn provide<T: PartialEq + 'static>(&mut self, value: T);
    pub fn get<T: 'static>(&mut self) -> Option<Rc<T>>;
}
```

- O `BuildContext` só existe durante um `Element::build`, então `provide` e
  `get` só podem ser chamados ali. As regras completas estão na seção 8.

### 6.6 `App`

```rust
pub struct App { /* raiz + sinal de "precisa atualizar" + envenenado */ }

impl App {
    pub fn new() -> Self;
    pub fn set_root(&mut self, widget: Rc<dyn Widget>);
    pub fn on_needs_update(&mut self, wake: impl Fn() + Send + Sync + 'static);
    pub fn needs_update(&self) -> bool;
    pub fn update(&mut self);
    pub fn debug_tree(&self) -> String;
}
// Default e Drop
```

- **`set_root`:** aplica as regras de atualização de filho (seção 7.2) à raiz
  e em seguida roda `update()`. Ao terminar, a árvore está construída.
- **`on_needs_update`:** registra o callback que pede um frame ao backend,
  substituindo o anterior. Se já houver atualização pendente no momento do
  registro, o callback novo é chamado uma vez.
- **`needs_update`:** é verdadeiro quando algum element foi marcado como sujo
  depois do último `update()`.
- **`update`:** reconstrói o que estiver sujo (seção 9).
- **`debug_tree`:** devolve uma linha por element, em pré-ordem, com dois
  espaços de recuo por nível de profundidade. Cada linha tem o nome curto do
  tipo do widget e, se houver key, ` #` seguido do `Debug` da key. O nome
  curto é o `type_name` sem os caminhos de módulo de cada segmento. Exemplo:

  ```
  App
    Group
      Item #"a"
      Item #"b"
  ```

- **`Drop`:** destrói a árvore, com os filhos sempre antes do pai. Os `Drop`
  dos `ElementCore` cancelam as assinaturas de notifier.

## 7. Árvore de elements

### 7.1 `ElementCore`

```rust
pub struct ElementCore {
    widget: Rc<dyn Widget>,
    children: Vec<Box<dyn Element>>,
    flags: Arc<Flags>,                                  // seção 9
    subscription: Option<(Arc<Notifier>, usize)>,       // assinatura de widget.notifier()
    provided: HashMap<TypeId, Provided>,                // seção 8
    dependencies: HashMap<TypeId, Option<Rc<dyn Any>>>, // o que o último build leu com get
}
```

- **Entrada na árvore:** o framework chama `widget.create_element()`, liga as
  flags do novo element às do pai e ao sinal do `App`, assina
  `widget.notifier()` (se houver) com `dirty_listener()`, e liga o `dirty`
  do element para que ele seja construído na mesma passada.
- **Marcação local:** marcações feitas pelo framework durante a passada
  (filho novo, widget trocado) só ligam o `dirty` do próprio element, sem
  propagar e sem acordar o backend. O pai está sendo visitado e vai olhar os
  filhos em seguida (seção 9.2).
- **`Drop` do `ElementCore`:** cancela a assinatura do notifier.

### 7.2 Atualizar um filho

**`can_update(antigo, novo)`** é verdadeiro quando:
- o `TypeId` do valor concreto é o mesmo (não o `TypeId` do `Rc`); e
- as keys são iguais (ambas `None`, ou iguais).

**`update_child(antigo, novo)`:**

| Antigo | Novo | Ação |
|---|---|---|
| nenhum | nenhum | nada |
| existe | nenhum | remove o antigo |
| nenhum | existe | cria o novo element (seção 7.1) |
| existe | mesma instância (`Rc::ptr_eq`) | nada. O element é mantido sem ser marcado como sujo |
| existe | `can_update` | troca o widget no core. Se o notifier mudou (`Arc::ptr_eq`, ou um dos dois é `None`), troca a assinatura. Liga o `dirty` (marcação local) |
| existe | outro tipo ou outra key | remove o antigo e cria o novo |

**Remover um element** destrói recursivamente os filhos, do último para o
primeiro, e depois o próprio element.

### 7.3 Reconciliação de listas (`reconcile.rs`)

`update_children(filhos_antigos, widgets_novos) -> filhos_novos` segue o
algoritmo do Flutter, movendo os `Box<dyn Element>` entre vetores:

1. **Validação:** se duas keys entre os widgets novos forem iguais, entra em
   pânico com a mensagem `key duplicada entre irmãos: {key:?}`.
2. **Topo:** enquanto antigo[i] e novo[i] forem compatíveis (`can_update`),
   aplica `update_child` e avança.
3. **Base:** do fim para o começo, enquanto os pares forem compatíveis, só
   avança, sem atualizar ainda.
4. **Miolo, lado antigo:** os antigos com key vão para um
   `HashMap<Key, Box<dyn Element>>`, e os sem key são removidos.
5. **Miolo, lado novo:** para cada widget novo, se ele tem key e o mapa tem
   um element com essa key e tipo compatível, o element sai do mapa e recebe
   `update_child`. Senão, um element novo é criado.
6. **Sobras:** os elements que restaram no mapa são removidos.
7. **Base:** aplica `update_child` aos pares da base, encontrados no passo 3.

## 8. Dados compartilhados: `provide` e `get`

### 8.1 Escopo

- **O escopo é um `Rc<HashMap<TypeId, ScopeEntry>>` imutável**, passado de
  pai para filho durante o `update`.
- Um element que forneceu valores cria, para os filhos, um mapa novo com o
  mapa do pai mais os seus valores. Um element que não forneceu nada repassa
  o mesmo `Rc`.
- O `build` de um element recebe o escopo do **pai**. Por isso um element não
  enxerga o próprio `provide`.
- `ScopeEntry` guarda o valor (`Rc<dyn Any>`) e o registro de dependentes do
  provedor.

### 8.2 `provide` e `get`

- **`provide::<T>(valor)`** grava `valor` no element que está construindo, com
  o tipo apagado e junto com uma função de igualdade de `T`. Se for chamado
  duas vezes no mesmo `build`, vale a última.
- **`get::<T>()`** procura `T` no escopo (O(1)) e devolve `Some(Rc<T>)` ou
  `None`. Ele também:
  - registra em `dependencies[T]` o `Rc` lido (ou `None`);
  - se encontrou um provedor, registra as flags do element no registro de
    dependentes desse provedor para `T`. O registro é um mapa indexado pelo
    endereço das flags, guardando `Weak<Flags>`, então reconstruções
    repetidas não o fazem crescer.
- **As `dependencies` de um element refletem só os `get` do último `build`.**
  Elas são limpas no início de cada `build`.

### 8.3 Depois do `build` de um provedor P

Para cada tipo T em `provided_antigo ∪ provided_novo`:

| Situação de T | Efeito |
|---|---|
| presente antes e depois, valores iguais (`PartialEq`) | mantém o `Rc` e o registro antigos. Nada é reconstruído |
| presente antes e depois, valores diferentes | o `Rc` novo substitui o antigo; os dependentes registrados são marcados como sujos; o registro é esvaziado (os dependentes se registram de novo ao reconstruir) |
| removido | os dependentes registrados são marcados como sujos. Eles vão resolver para um provedor acima ou para `None` |
| adicionado | a subárvore de P é visitada inteira nesta passada. Cada element visitado compara, para cada tipo em `dependencies`, o `Rc` registrado com o que o escopo tem agora (presença e `Rc::ptr_eq`). Se algo mudou, ele é reconstruído |

- **Marcar dependentes durante a passada** usa uma propagação limitada: ela
  liga `dirty` no dependente e `descendant_dirty` nos ancestrais dele **que
  estão abaixo de P**, sem tocar nas flags de P e sem acordar o backend. P já
  desligou as próprias flags e vai olhar os filhos em seguida (seção 9.2).
  Os dependentes são sempre descendentes de P, então são alcançados na mesma
  passada.
- **Registros apontando para elements já destruídos** (`Weak` morto) ou que
  deixaram de ler T causam, no máximo, uma reconstrução extra. Eles somem
  quando o registro é esvaziado.

## 9. Reconstrução sob demanda e threads

### 9.1 Flags e sinal (`flags.rs`)

```rust
struct Flags {
    dirty: AtomicBool,
    descendant_dirty: AtomicBool,
    parent: OnceLock<Arc<Flags>>,  // definido quando o element entra na árvore; a raiz não tem
    signal: OnceLock<Arc<Signal>>, // o sinal do App
}

struct Signal {
    pending: AtomicBool,
    wake: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}
```

- **`mark_dirty`** (qualquer thread):
  1. liga `dirty`;
  2. sobe pelos `parent` ligando `descendant_dirty` com `swap`, e para no
     primeiro que já estava ligado;
  3. chama `signal.request()`.
- **`signal.request()`:** se `pending` estava desligado, liga e chama `wake`
  (clonado sob o lock e chamado fora dele). O backend é acordado uma vez por
  transição de "nada pendente" para "pendente".
- **Ordem de memória:** todas as operações nas flags usam `SeqCst`. O mutex
  de `wake` ignora envenenamento, como no `reactive-rs`, porque nenhum código
  de usuário roda com ele travado.

### 9.2 `App::update()`

1. Entra em pânico se o `App` estiver envenenado (seção 10). Marca como
   envenenado até terminar.
2. `signal.pending = false`.
3. Visita a raiz com o escopo vazio.
4. Desmarca o envenenamento.

**Visitar um element E com escopo S** (na thread de interface):

1. `reconstruir = E.dirty.swap(false)`, ou a visita é completa e alguma
   dependência de E mudou (seção 8.3).
2. `E.descendant_dirty.swap(false)`. A flag é desligada **antes** de olhar os
   filhos.
3. Se `reconstruir`:
   - chama `E.build` com escopo S;
   - aplica as regras de `provide` (seção 8.3);
   - reconcilia os filhos (seções 7.2 e 7.3). Filhos novos e filhos com
     widget trocado ficam marcados como sujos.
4. Calcula o escopo dos filhos (seção 8.1).
5. Para cada filho, em ordem, visita se `dirty` ou `descendant_dirty`
   estiverem ligados, ou se esta for uma visita completa.

### 9.3 Garantias

- **Pai antes do filho, cada element construído no máximo uma vez por
  passada.** A descida é de cima para baixo e `dirty` é desligado ao visitar.
- **Galhos limpos não são visitados.** O custo do `update` é proporcional aos
  caminhos até os elements sujos, exceto na visita completa da seção 8.3.
- **Nenhum aviso se perde.**
  - Se um `mark_dirty` concorrente acontece depois que a passada já passou
    por um ancestral, a propagação liga de novo as flags até a raiz e
    `signal.request()` acorda o backend para o próximo `update()`.
  - Se acontece antes, a parada antecipada só ocorre num ancestral ainda não
    visitado, que esta passada ainda vai visitar.
- **Agrupamento:** vários avisos para o mesmo element entre dois `update()`
  resultam em uma reconstrução.
- **Avisos durante o `update`,** inclusive de código de usuário na própria
  thread, ficam para o próximo `update()`, e o backend é acordado. Um `set`
  incondicional dentro de um `build` gera uma reconstrução por frame. A
  documentação de `BuilderWidget` menciona isso.
- **Threads:** `App`, `ElementCore`, `BuildContext`, `Rc<dyn Widget>` e `Key`
  vivem na thread de interface. Por conterem `Rc`, não são `Send`. O que
  atravessa threads são `Arc<Flags>`, `Arc<Signal>` e os listeners, todos
  `Send + Sync`.

## 10. Erros

- **Erros de programação** causam pânico com mensagem clara:
  - key duplicada entre irmãos;
  - `widget_as` com o tipo errado;
  - `BuilderWidget::set` sem notifier.
- **Envenenamento.** Um pânico em código de usuário durante `set_root` ou
  `update` (em `build`, `create_element` ou no `Drop` de um element) deixa o
  `App` envenenado. A árvore pode ter ficado incompleta. A partir daí,
  `set_root` e `update` entram em pânico com
  `App envenenado por um pânico anterior`. Não há recuperação nesta parte.
- **`Drop` do `App`** se comporta igual com ou sem envenenamento: destrói o
  que restou da árvore, filhos antes do pai. Não chama nenhum método de
  `Element`.

## 11. Testes

Os testes ficam ao lado de cada módulo, como no `reactive-rs`.
`test_support.rs` fornece:
- um `Log` thread safe que registra eventos;
- um `Leaf` (`Widget` sem filhos);
- um `Group(Vec<Rc<dyn Widget>>)`, um `Widget` com vários filhos e element
  próprio, que exercita a reconciliação de listas;
- um `Probe`, um `Widget` cujo element registra no `Log` a criação
  (`create_element`), cada `build` e o `Drop`.

O `App` expõe, só em `cfg(test)`, quantos elements a última passada visitou.

Cenários:

1. **Montagem:** `set_root` constrói a árvore inteira, conferida pelo
   `debug_tree`.
2. **Atualização:**
   - mesmo tipo e mesma key reaproveita o element (`build` sem nova criação);
   - outro tipo ou outra key cria um element novo e destrói o antigo;
   - a mesma instância não reconstrói o element.
3. **Listas com key:**
   - reordenar mantém cada element com sua key;
   - inserir e remover no começo, no meio e no fim;
   - filhos sem key no miolo são recriados;
   - key repetida causa pânico.
4. **Destruição:** filhos antes do pai, tanto ao remover um element quanto no
   `Drop` do `App`.
5. **`BuilderWidget`:**
   - `set` roda a mudança e notifica;
   - no próximo `update()`, só aquele element é reconstruído;
   - um aviso vindo de outra thread chama o callback de acordar uma vez por
     transição;
   - vários avisos geram uma reconstrução;
   - trocar de notifier troca a assinatura;
   - um element removido não acorda mais o `App`;
   - `set` sem notifier causa pânico.
6. **Flags:**
   - um aviso num element profundo faz a passada visitar só o caminho até
     ele (contador de visitas);
   - teste de estresse: avisos de várias threads durante `update()`
     sucessivos. No fim, com `needs_update() == false`, todo element avisado
     foi reconstruído depois do seu último aviso.
7. **`provide`/`get`:**
   - encontra o provedor mais próximo e não enxerga o próprio `provide`;
   - valor igual não reconstrói ninguém;
   - valor diferente reconstrói só os dependentes, e irmãos que não leram o
     valor não são reconstruídos;
   - um provedor que deixa de fornecer reconstrói os dependentes, que passam
     a ver o provedor acima ou `None`;
   - um provedor que passa a fornecer no meio da árvore reconstrói quem
     estava resolvido acima ou em `None`;
   - um dependente reconstruído muitas vezes por outro motivo não faz o
     registro do provedor crescer;
   - um element que deixou de chamar `get` não é mais reconstruído por
     mudanças daquele tipo depois que o registro é esvaziado.
8. **Element personalizado:** um element escrito como outra biblioteca faria
   (com `ElementCore` embutido) assina um notifier externo com
   `dirty_listener()` e é reconstruído quando esse notifier avisa de outra
   thread.
9. **Erros:**
   - um pânico em `build` envenena o `App` e as chamadas seguintes entram em
     pânico;
   - o `Drop` de um `App` envenenado cancela as assinaturas: o notifier avisa
     e nada é acordado.
10. **`reactive-rs`:** `ValueListenable::get` funciona por
    `&dyn ValueListenable<T>`.

## 12. Fora desta parte, para as próximas

- **Parte 2:** ganchos de render e layout no `Element` (métodos com
  implementação padrão), acesso mutável aos filhos, e as flags
  `needs_layout`/`needs_paint` seguindo o mesmo padrão de propagação.
- **A partir da parte 2:** widget de erro no lugar de uma subárvore que entrou
  em pânico.
- **Parte 6:** um registro de foco, já que não há acesso direto a um element
  por id.
- **Sem prazo:** `GlobalKey`, widgets assíncronos e uma macro para reduzir o
  código repetitivo de `BuilderWidget`.
- **Nome:** se a crate for publicada no crates.io, avaliar o nome
  `notify-rs`, que pode ser confundido com a crate `notify` (watcher de
  arquivos).
