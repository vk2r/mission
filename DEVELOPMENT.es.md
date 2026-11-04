# Notas de desarrollo

[English](DEVELOPMENT.md) | Español

El uso está en [README.es.md](README.es.md); este archivo cubre lo que necesitas saber para modificar el plugin.

Mission es un widget de barra que pone a un equipo de agentes de IA en la barra de Omarchy: sus estados, su trabajo y su gasto. Los datos vienen de la app de agentes con la que el widget está emparejado — hoy esa app es [MausBot](https://mausbot.com/) — y el motor solo lee los archivos locales de esa app. Cuando un comportamiento depende del formato o de datos propios de la app, este documento lo dice; nada más en el diseño supone una app concreta.

## Nombres e identificadores

La marca de la app es **MausBot** (título de la ventana, https://mausbot.com/), y todos los textos que lee el usuario lo dicen así. Los identificadores técnicos todavía dicen `openmausbot` a propósito: la clase de Hyprland `com.openmausbot.app`, el lanzador `openmausbot.desktop`, `~/.openmausbot` y `~/.config/openmausbot`, el espacio de trabajo especial `openmausbot`. Son la forma en que el plugin encuentra, abre y oculta la app y donde guarda su configuración, así que renombrarlos rompería la detección. El id del plugin es `io.github.vk2r.mission` y su carpeta de runtime es `$XDG_RUNTIME_DIR/mission`.

## Preparación

Las herramientas de compilación están fijadas en `mise.toml`: la cadena de herramientas de Rust, `cargo-deny` para la auditoría de dependencias y `cargo-about` para el aviso de licencias de terceros.

```sh
mise install            # Rust, cargo-deny, cargo-about
mise run build          # recompila bin/engine
mise run test           # tests del motor + la comprobación de traducciones
mise run audit          # avisos, prohibiciones, licencias y fuentes de cada crate
mise run licenses       # regenera licenses/THIRD-PARTY.txt (tras cambiar dependencias)
mise run validate       # omarchy plugin validate .
```

```sh
mise run fixtures        # regenera engine/tests/fixtures (fechas relativas a ahora)
mise run i18n            # solo la comprobación de traducciones
UPDATE_GOLDEN=1 cargo test --bin engine golden   # desde engine/: regenera engine/tests/golden tras un cambio de dibujo intencionado
```

- Para trabajar desde un checkout: enlaza el proyecto en la carpeta de plugins con el propio id del plugin — `ln -s ~/Projects/Software/mission ~/.config/omarchy/plugins/io.github.vk2r.mission` — luego añade **Mission** a la barra desde los ajustes de barra de Omarchy y reinicia la shell una vez (`omarchy restart shell`). Omarchy instala los plugins con un `git clone` simple y nunca compila, así que este es solo el camino de desarrollo.
- Cargo compila en `~/.cache/openmausbot-engine/target`, fuera de la carpeta del plugin, porque Omarchy recarga el plugin cada vez que cambia un archivo dentro de él.
- `bin/` solo contiene `engine`. Omarchy instala los plugins con un `git clone` simple y nunca compila, así que el binario está versionado y CI lo construye (ver [Binarios y releases](#binarios-y-releases)); un `mise run build` local es solo para pruebas. Ejecuta `git config core.hooksPath .githooks` una vez por clon: su hook `pre-commit` rechaza commits que toquen `bin/engine`, `bin/SHA256SUMS` o `bin/provenance.sigstore.json`, así que descarta una compilación local con `git restore --staged --worktree bin/engine`.
- Versiona `engine/Cargo.lock`, y `licenses/THIRD-PARTY.txt` tras `mise run licenses`, cuando cambien las dependencias.
- El JSON que imprime el motor es el contrato con el QML (`EngineHub.qml` → `BarWidget.qml` / `Panel.qml`); conserva su forma al cambiar el motor. `engine/tests/daemon.rs` lo verifica.

## Las piezas

| archivo                            | qué es                                                                                                      |
| ---------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `manifest.json`                    | manifiesto del plugin (`bar-widget`)                                                                        |
| `qmldir`                           | registra el único singleton de motor para la carpeta propia del plugin                                      |
| `BarWidget.qml`                    | el icono, su respiración y el cargador del panel; dibuja, nunca consulta en bucle                          |
| `EngineHub.qml`                    | `pragma Singleton`: un `bin/engine` para toda la shell, y la única instantánea que lee cada widget          |
| `Panel.qml`                        | el anfitrión del panel: la tarjeta, el teclado, la página en vista y las palabras que usan todas las páginas |
| `panel/Home.qml`                   | la página del equipo: la jerarquía y el pie (el gasto en una frase y el botón de medidor)                    |
| `panel/Settings.qml`               | idioma, el período de costo, notificaciones y las opciones del widget                                       |
| `panel/Costs.qml`                  | el costo del período elegido: total, línea de tendencia, barra dividida al 100 % y la lista por bot o por modelo |
| `components/BotRow.qml`            | un bot en la página del equipo: mascota, nombre, trabajo actual, punto y palabra de estado                   |
| `components/RangePicker.qml`       | el selector de período compartido (página de Ajustes y cabecera de la página Medidor)                       |
| `mascot/motion.js`                 | la tabla de movimiento por cara y el ritmo de parpadeo de la app (la de su barra lateral), usada por `BotRow.qml` |
| `components/BrandIcon.qml`         | la marca, teñida desde el SVG en `assets/` para la barra y el panel                                         |
| `components/CompactDropdown.qml`   | el selector pequeño que usa la página de Ajustes                                                            |
| `lang/index.js`, `lang/*.js`       | los ocho idiomas (`index.js` es el cargador); `en`, `es`, `de`, `fr`, `it`, `nl`, `pl`, `pt`                |
| `mascot/mascot.json`               | el catálogo de mascotas de la propia app: diez cuerpos y veinticinco caras, extraído de la app instalada    |
| `engine/`                          | las fuentes en Rust de `bin/engine` (demonio, roster, mascotas, Hyprland, seguidor de registro, gasto)      |
| `bin/engine`                       | el binario que ejecuta la barra: una línea JSON por cambio, órdenes por stdin                               |

## El motor

`bin/engine` sin argumentos es el demonio que ejecuta la barra: una línea JSON por cambio en stdout, órdenes (`refresh`, `show`, `hide`, `toggle`, `ack`) en stdin, y EOF en stdin lo termina — así un widget retirado de la barra no deja ningún proceso detrás. Una línea lleva `v`, `ok`, `window`, `alert`, `busy`, `working`, `bots` y `costs`.

- **Una lectura por fuente.** Un tic cuesta un `PRAGMA data_version` sobre la base de datos de la app y unas pocas llamadas `stat()`; las respuestas se reconstruyen solo para la fuente que cambió, y un tic que no encuentra nada no dice nada.
- **Gasto.** `usage.rs` suma `usage/YYYY-MM.jsonl` sobre tres ventanas que terminan hoy: `day`, `week` (los últimos 7 días) y `month` (los últimos 30), con un margen de dos días al elegir los archivos a leer, porque un registro con sello en UTC puede caer en otra fecha local en una zona horaria adelantada. Cada rango lleva también `series`, una barra por hora del día o por día de la ventana, llenada en la misma lectura para que las barras y el total no puedan discrepar, y dos particiones del mismo dinero, `byBot` y `byModel`: el modelo es el campo `model` del registro, recortado, sin caracteres de control y cortado a 60 bytes, para que una línea malformada no estire una fila; cada fila de `byModel` nombra además al bot vivo que más gastó en él (`bot`, ausente cuando solo bots eliminados o sin nombre lo usaron), lo que la página de medidor convierte en el color de la fila. Una línea corrupta se salta, nunca es fatal.
- **El registro.** `logwatch.rs` sigue el registro de la app para lo único que tiene que llegar al momento: un turno entre bots, que sella el aviso y lanza la notificación. El demonio no guarda lista de lo que leyó; tras un reinicio arranca igualmente al final del archivo, así que nada antiguo se vuelve a anunciar.
- **Mascotas.** Cada fila se dibuja en cada lectura desde el registro del bot; `engine/tests/golden` guarda un resumen por dibujo, los pivotes y diez SVG de muestra; `UPDATE_GOLDEN=1 cargo test --bin engine golden` los regenera tras un cambio intencionado.
- **Sin código unsafe.** El crate es `#![forbid(unsafe_code)]`; `mise run audit` cubre avisos, licencias y fuentes.

- **Hyprland a través de su socket.** `hypr.rs` pide a la propia socket de peticiones de Hyprland (`$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket.sock`, la que usa `hyprctl`) `j/clients` y `j/monitors`, y recurre a `hyprctl -j` cuando la socket no se puede alcanzar o no responde con una lista. El demonio se conecta solo como cliente; no escucha en nada.

En reposo el demonio gasta cerca del 0,012 % de un núcleo y 7,8 MiB, sin procesos hijo: se pregunta a Hyprland por su socket, las sentencias están cacheadas y un tic que no encontró nada no construye línea.

Para observar el demonio tú mismo:

```sh
bin/engine --once                     # la línea, una vez, en una terminal
bin/engine                            # líneas a medida que cambia algo; Ctrl-D lo termina
printf 'hide\nshow\n' | bin/engine     # las órdenes entran por stdin, y el cierre del tubo lo termina
```

El reloj se puede mover para una prueba — `bin/engine --once --now 2026-11-02T00:00:05-03:00` — que es como se comprueba la medianoche y los cinco minutos de gracia de `failed` sin esperarlos.

## El registro de uso

- **Cómo escribe la app `usage/YYYY-MM.jsonl`:** solo añade al final. El `usage-ledger.js` de la app escribe cada registro con `appendFile(file, JSON.stringify(record) + "\n", { mode: 0o600 })`, a través de una única cola de escritura por directorio de datos, y nada más en la app instalada abre esos archivos para escritura (`fleet-cli` y `group-thread-usage` solo leen). Así `ledger.rs` confía en el inodo, la longitud y la guarda de 4 KiB antes del antiguo final; la relectura completa por tiempo (`RECHECK`) es ahora de 30 minutos y es solo una red de seguridad para un archivo que alguien editó a mano más atrás de donde llega la guarda. Antes eran 60 s contados desde la última lectura *completa*, así que cerca de la mitad de los cambios reales (un registro cada ~42 s) releían el mes entero. Medido con un mes de 6 MB, 12 añadidos separados 42 s: 6 de 12 cambios leían 6,1 MB antes, 0 de 12 ahora (61 KB cada uno, todo de la consulta a Hyprland).
- **Deliberadamente no hecho (deuda):** una comprobación de hash de bloques de la parte ya leída (no hace falta mientras la app solo añade al final); #7 cachear la zona horaria por tic (`TimeZone::system()` cuesta microsegundos, y los cambios de `TZ` se recogen al momento); #9 un registro de páginas en `Panel.qml` (añadir una página sigue tocando `page`, `goTo`, `contentHeight` y la visibilidad de cada página); N4 fecha/hora local por registro (la agregación recorre el mes, ~3 ms con 6 MB); #8 cachear el JSON de `costs`.
- **La línea:** las barras de `series` se redondean a seis decimales, `variants[0]` (el dibujo propio de la fila) sale vacío, y las filas de `byBot`/`byModel` llevan `botId` para que el panel colorea por id, no por nombre. Efecto neto en `--once`: de 14,7 KB a 15,2 KB.
- **Avatares:** la URL de un dibujo se recuerda junto a su SVG y se olvida cuando `prune` ha borrado algún archivo; un archivo ya nombrado como un dibujo se reutiliza solo si es un archivo regular de la misma longitud.
- **Ajustes:** `notifyOn` lee `0`, `"no"`, `"off"` y `null` como apagado, `EngineHub.save` copia la entrada viva a través de JSON (una clave `__proto__` propia sobrevive), y un sello dejado legible por otros se estrecha a 0600 al leerse. La prueba de distribución de teclado vive una sola vez en `EngineHub`, no en cada widget.

## Un motor para todas las pantallas

`EngineHub.qml` es un singleton de QML (registrado en `qmldir`). Esta shell ejecuta una barra, y esa barra construye un widget por monitor dentro de un único motor QML, así que el singleton es un objeto para todas las pantallas, y es el dueño del único proceso `bin/engine`: `snapshot`, `state`, `alerting`, `teamBusy`, `costs` y el reloj de animación `phase` los leen ambas barras de los mismos valores, y nunca arranca un segundo motor.

Los widgets se registran en el hub (`attach`/`detach`); cuando se va el último, el motor se detiene, así que un widget retirado de la barra no deja ningún proceso detrás. Las órdenes que envían los widgets — `show`, `hide`, `toggle`, `ack`, `refresh` — vuelven por el stdin del motor, así que un clic no arranca ningún proceso.

Dónde está la ventana y si hay un aviso esperando lo responden archivos de runtime bajo `$XDG_RUNTIME_DIR/mission/`: `alert` guarda el evento más nuevo y `seen` el que despejaste con un clic, y esos sellos son los que fijan el estado de aviso mientras difieran. La carpeta sobrevive a un reinicio de la shell; el hub es lo que hace que dos pantallas se pongan de acuerdo sin preguntar dos veces.

El seguidor arranca al **final** de `server.log`, porque un reinicio de la shell no debe reproducir el historial del registro como una ráfaga de notificaciones. Si el registro se rota o se trunca, salta al final del archivo nuevo en vez de releerlo desde el principio.

## Páginas

`Panel.qml` es el anfitrión: posee la tarjeta, el teclado, el pie de versión, los datos que lee cada página y las palabras. Una página es un archivo en `panel/` que recibe el anfitrión como `required property var host` y se instancia en la pila de `Panel.qml`. `page` guarda el nombre en vista (`home`, `settings`, `costs`); un control abre una página con `root.goTo("…")` — Ajustes desde el engranaje de la cabecera, la página de medidor desde el botón del pie — y Esc o Backspace vuelven a `home` desde cualquier otra.

Inicio está dibujado todo el tiempo y solo se funde, y la tarjeta conserva la altura de Inicio sea cual sea la página en vista, así que nada salta al moverse entre páginas. La página de medidor llena esa altura en lugar de hacer scroll a la tarjeta: su total, su línea de tendencia y su barra dividida son fijos y solo la lista es un `Flickable`, en la altura que deja la parte fija. El pie de versión se dibuja solo en Ajustes. Para añadir una página: un archivo nuevo en `panel/`, una entrada en la pila de `Panel.qml`, un glifo (`String.fromCodePoint`, buscado por nombre en la Nerd Font instalada) y sus claves en `lang/*.js`.

Una página que muestra datos que envía el motor los lee a través del anfitrión, nunca de la línea misma: `rangeReport()`, `costText()`, `costSentence()`, `costSeries()` y `costBreakdown()` son del anfitrión, así que el pie y la página de medidor hablan siempre del mismo rango, y de la lista de la partición elegida en su fila LIST.

## Clics, avisos y gasto (lo que ve el usuario)

- **El clic izquierdo** abre el panel; otro clic lo cierra, y abrirlo también limpia el aviso (`Panel.qml` `open()` hace ack mientras `alerting`).
- **El clic derecho** envía la orden `show` del motor: en pantalla la ventana se eleva y se enfoca; estacionada se devuelve al espacio de trabajo que estás viendo y se enfoca; sin correr, se lanza la app desde su entrada `.desktop`. **El clic central** envía `toggle`, que estaciona la ventana en un espacio de trabajo especial oculto sin ruido (lo que estuvieras escribiendo conserva el teclado) y la devuelve si no; también lanza la app cuando falta. La orden IPC `toggle` abre y cierra el panel — usa el IPC `show` o `hide` para mover la ventana.
- **Aviso:** cuando un bot responde a otro bot, o el resultado de un sub-bot vuelve a su padre, una notificación de escritorio lleva la primera línea de ese turno y el tooltip dice que hay algo nuevo. La marca misma no lleva insignia: mientras el equipo trabaja respira, y ese es el único cambio en la barra. Un clic es la forma de decir que lo viste, así que hacer clic también limpia el aviso. Los turnos que el usuario mismo inicia no se anuncian ni se listan: ya sabes lo que escribiste.
- **Las cifras de gasto** vienen del registro de uso de la app (`~/.openmausbot/usage/YYYY-MM.jsonl`, un registro por turno). El rango es el día, la semana o el mes natural local; el registro lleva sello y se archiva en UTC, así que también se leen los meses de los lados y cada registro se coloca por su propia fecha local. El costo suma `costUsd` sobre todos los registros, trabajo en segundo plano incluido; una tarea es un turno cuyo resultado es `ok`, quienquiera que lo iniciara. Nada se cachea, así que los costos que otro script rellene después aparecen en el siguiente cambio.
- El texto de la notificación se limpia de caracteres de control y bidireccionales y se corta a 120 bytes en un límite UTF-8, para que un turno largo no pueda romper la notificación ni el protocolo de líneas.
- Nada toca `~/.config/hypr/windows.lua` ni `hyprland.lua`: el espacio de trabajo especial se usa a través de dispatches, sin reglas de ventanas.

## El dibujo de la marca

El icono de la barra es la marca propia del plugin, `assets/mission-mark.svg` (el parche «A» del Apollo, versión de barra: una estrella de cuatro puntas y una órbita inclinada, sin anillos), dibujado por `components/BrandIcon.qml` con el método que usan los iconos de marca de la propia shell: el SVG se lee como texto, se sustituyen sus dos tokens de color y el resultado va a un `Image` como URL de datos — sin un segundo archivo por tema ni efecto de GPU. `currentColor` (las estrellas) toma el color de texto del tema y `accentColor` (la órbita) el acento del tema, tanto en `fill` como en `stroke`, así que la marca sigue un cambio de tema en vivo. La sustitución es global a propósito: `String.replace` con un patrón simple se detiene en el primer acierto, y en un SVG comentado el primer acierto es el comentario. La alfa es una opacidad de grupo alrededor del dibujo, no un atributo por elemento, así que nunca choca con la `opacity` que la obra lleva para su jerarquía. Ambos colores retroceden al 45 % mientras la ventana está estacionada. El dibujo mide 14 px con 1 px de aire por lado, dentro de un `viewBox` de 16 px: la marca ocupa toda la caja de 16 px que la shell da a cada icono de barra, así que el hueco conserva sus 27 px y los vecinos conservan su sitio. La cabecera del panel dibuja `assets/mission-mark-hero.svg`, la versión de 48 px con trazos más finos, a 36 px bajo el título «Mission» y la línea «A MausBot Companion».

Mientras un bot está en un turno la marca también respira — un pulso suave de opacidad, 650 ms por camino, a partir del contador `busy` del motor — y se queda quieta cuando el equipo está inactivo. El pulso lee el `phase` del hub, así que ambos monitores respiran en el mismo ritmo en lugar de guardar cada uno una copia de la animación. Con el panel abierto responden las filas del propio panel, así que el hub nunca pregunta una segunda vez.

La corona junto al nombre del jefe, el robot al que cae una fila cuando el catálogo no puede dibujar un bot, y el engranaje, el historial y la flecha de la cabecera son glifos construidos con `String.fromCodePoint`, porque los codepoints de Material Design viven en el plano 15 y un escape `"\uXXXX"` lleva exactamente cuatro dígitos hexadecimales: `"\uf037d"` es U+F037 más la letra `d`, que es lo que antes se dibujaba como basura. Cada codepoint de aquí se buscó por nombre en la JetBrainsMono Nerd Font instalada, no en una tabla de MDI.

## La jerarquía del equipo

- Quién dirige es un dato propio de la app, no un nombre guardado aquí: la app marca su Chief of Staff con `chiefOfStaff: true` en el registro de ese bot en `bots.json`, y el motor ordena con ello — el jefe primero, luego el resto del equipo en el orden en que `bots.json` los tiene, así que el panel nunca discrepa del roster propio de la app. Mueve esa marca en la app y el panel la sigue: con ella en Designer, Designer toma la tarjeta y Boss cae debajo del tronco. La marca es un indicador por bot, no un «el» jefe: si más de un registro la lleva, cada uno recibe su propia tarjeta sobre el tronco. Un roster sin ningún jefe se queda en el orden del roster, sin tronco ni encabezado.
- El grupo bajo el jefe es el departamento que la propia app guarda: el campo es `section` en el registro del bot (un `department` antiguo se acepta todavía, y después deja de hacerlo), y un bot sin sección se sienta en el departamento por defecto de la app, cuyo nombre es la etiqueta de reserva de la propia app — «General» (sus textos: _Uncheck a bot to move it to General_). La app mantiene a su jefe dentro de esa sección — su propia comprobación de respaldo rechaza dos jefes visibles _en una misma sección_.
- Ningún encabezado se dibuja sobre ninguno de los dos grupos, a propósito. La primera versión rotulaba el grupo inferior con el departamento y cuántos bots hay en él (el mapa de la app lee `GENERAL · 5 bots`); VK lo encontró redundante bajo la propia fila del jefe. La etiqueta `CHIEF OF STAFF` sobre la tarjeta se fue después, por una razón estructural: la marca puede estar en más de un bot, así que una etiqueta que nombra el rol se repetiría. La jerarquía la llevan ahora solo la corona, el marco y el tronco — y el tronco empieza en el borde inferior externo de la tarjeta del jefe, así que el equipo cuelga de la cabeza en vez de flotar debajo. Cada fila lleva todavía su departamento desde `bots.json`, así que un encabezado puede volver de los datos si alguna vez se quiere.

## Renderizado de mascotas

- La mascota de cada fila se dibuja en cada lectura desde el registro de ese bot en `bots.json`: `mascotBody` es el cuerpo, `color` el relleno, `mascotExpression` la cara, y un bot sin expresión lleva la cara que pide su estado en vivo, o la que sugiere su rol. Cambia el aspecto de un bot en la app y la barra lo sigue en el siguiente cambio — no hay imagen que regenerar ni manifiesto que mantener al día. Un cuerpo desconocido recurre al cursor por defecto del propio catálogo, en plata; sin catálogo por completo, cada fila recurre al glifo de robot y el roster se dibuja igualmente.
- Cada mascota son dos imágenes sobre un mismo objetivo: el cuerpo con su boca, y los ojos solos, ambos dibujados desde la misma `face_geometry()` sobre el mismo viewBox, más el centro de los ojos como fracción de ese cuadrado. Eso es lo que permite a la barra parpadear los ojos — aplastados en su propia línea — sin mover la boca ni el cuerpo, y es la razón por la que los dos archivos se componen de vuelta exactamente en la imagen que la fila dibujaba antes de la división (comprobado píxel a píxel con `rsvg-convert`). Una lectura que no lleva capa de ojos hace que la fila se salte el parpadeo.
- El dibujo es un port del renderizador de la propia app, no una imitación: mismo viewBox y margen de 15 unidades, mismo gradiente de tres paradas, mismos anillos de ojos y curva de boca, misma transformación de colocación. Las veinticinco expresiones se compararon contra las funciones de la propia app ejecutadas directamente desde el bundle y salen idénticas carácter a carácter; `engine/tests/golden` guarda un resumen de cada dibujo.
- Las imágenes se escriben en `$XDG_RUNTIME_DIR/mission/team-avatars/` bajo un nombre hecho de su contenido. Un bot que cambió es una URL cambiada, así que Qt no puede servir una copia vieja, y un bot que no cambió conserva su archivo. Los archivos a los que nada apunta se borran tras un minuto, lo que deja solo el que el panel puede seguir dibujando.

## Movimiento de los avatares

Un bot trabajando (o notificando) se mueve como en la barra lateral de la app: `mascot/motion.js` guarda la propia tabla por cara de la app (bundle 0.1.136, `BD`/`Dze`) y sus pausas de parpadeo; `BotRow.qml` la muestrea con un `Timer` solo mientras el panel está abierto. `scripts/check-motion.mjs` fija las fórmulas y que cada cara de `mascot.json` tiene una entrada; `scripts/check-settings.mjs` fija que `EngineHub.save` conserva las otras claves de la entrada viva. La cara también cambia dentro de su familia como en la app (los tiempos de permanencia `oSe` en `FACE_DWELL`; el motor publica el `variants[]` de cada fila, un fundido hace de morfo de resorte de la app). `scripts/check-silent.sh` prueba que `--no-notify` produce cero llamadas a `notify-send`.

## Ajustes e idiomas

Las traducciones viven en `lang/`, un archivo por idioma, y `lang/index.js` las importa y guarda la lista. QML no puede resolver `import "lang"` hacia un `index.js` (una importación de carpeta busca un módulo QML con un `qmldir`), así que las importaciones son `import "lang/index.js" as I18n` (y `"../lang/index.js"` desde `panel/`). Igualmente, `mascot/motion.js` está junto a `mascot.json`. Sin elección, el idioma sigue la distribución de teclado de Hyprland, luego `LC_ALL` / `LC_MESSAGES` / `LANG`, luego el inglés.

Para añadir un idioma: copia `lang/en.js`, traduce cada valor, añade el `.import` y la entrada en `LANGUAGES` de `lang/index.js`, mapea sus distribuciones de teclado en `KEYBOARD_LANGUAGES`, y añade el código a `expectedCodes` en `scripts/check-i18n.mjs`. Luego `mise run i18n`, que comprueba que cada idioma tiene las mismas claves, que cada clave es una cadena (o las tres formas de un plural, como tiene el polaco), que las reglas de plural eligen la correcta, y que el QML lee realmente cada clave — una clave que nadie lee hace fallar la comprobación.

Los ajustes se guardan en la entrada del widget en `~/.config/omarchy/shell.json`: `language` (`auto` es el valor por defecto), `notify` (solo se guarda como `false`) y `costRange` (`day`, `week` o `month`; `day` es el valor por defecto), fijados en la página de Ajustes, y `costGroup` (`bot` o `model`; `bot` es el valor por defecto), fijado en la fila LIST de la página de medidor. Un valor por defecto se guarda como nada, así que una entrada que nadie tocó se queda `{"id": …}`, y Omarchy conserva la entrada a través de un reinicio de la shell.

Una frase que cuenta algo es una sola traducción, no piezas pegadas: `costSentenceDay`, `costSentenceWeek` y `costSentenceMonth` son plurales (tres formas en polaco) con los marcadores nombrados `{cost}` y `{tasks}`, y `I18n.trp()` elige la forma y los rellena. El pie los escribe en mayúsculas y corto («$36.65 HOY · 96 TAREAS», como máximo 31 caracteres en cualquier idioma) para que la frase y el botón de medidor compartan una línea. `scripts/check-i18n.mjs` falla cuando una forma pierde un marcador. Cada idioma lleva además `monthsShort`, sus doce nombres de mes cortos, para que ninguna fecha que el panel escriba tome prestada la configuración regional de la máquina (la línea de tendencia de la página de medidor no tiene eje de fechas, así que hoy nada las usa).

## Comportamiento que conviene conocer

- **El texto de la tarjeta es primero el brief.** Para un bot delegado, `roster.rs::task_for` muestra el brief que escribió su delegador (la orden del jefe), y `origin.quote` (las propias palabras de VK) solo cuando no hay brief. El jefe (Boss) viene de VK directamente y no tiene brief, así que su tarjeta no cambia.
- **Bots eliminados en la lista de costos.** `usage.rs` recibe los ids y los nombres actuales de `bots.json` (las filas se agrupan por `botId` y se nombran como la app nombra hoy al bot, así que un renombre no divide una fila); el gasto cuyo `botId` no está se añade a una única fila `deleted: true` (el panel la nombra «Bots eliminados», punto gris) y una fila que se redondea a $0.00 se deja fuera, su dinero reportado como `hiddenUsd` (los modelos tienen la misma regla: `hiddenModelUsd`). Las filas visibles más `hiddenUsd` son el total. Si `bots.json` no se puede leer o está vacío, nadie cuenta como eliminado. El sello del roster (ya vigilado) relanza la lectura de costos cuando cambia; nada nuevo se consulta en bucle.

## Por qué está escrito así

- `ledger.rs` abre los archivos de mes con `O_NOFOLLOW | O_NONBLOCK` y exige un archivo regular, así que un symlink o un FIFO plantado en `usage/` no puede redirigir ni bloquear una lectura.
- El registro confía en inodo, longitud y una guarda de 4 KiB antes del antiguo final porque la app solo añade al final; la relectura completa de 30 minutos es una red para ediciones a mano.
- `paths::write_atomically` escribe un archivo temporal y lo renombra, así que un lector nunca ve la mitad de un sello o de un avatar.
- Los archivos que el motor lee de las carpetas de la app se abren con `paths::READ_FLAGS` (`O_NOFOLLOW | O_NONBLOCK`), así que un symlink o un FIFO plantado se rechaza o es inofensivo.
- `launch.rs` arranca la app desde el `Exec` del archivo .desktop como vector de argumentos, nunca a través de una shell, en su propio grupo de procesos.
- `logwatch.rs` toma un bloqueo en la carpeta de runtime para que dos motores nunca anuncien el mismo turno, y estrecha un bloqueo antiguo a 0600.
- `usage::clean` limpia caracteres de control y bidireccionales de cada nombre y texto que vino de un archivo, antes de que llegue a una notificación o al panel.
- `stream.rs` ejecuta cada tic bajo `catch_unwind` y reporta un pánico como línea de error, así que una mala lectura no termina el demonio.
- El perfil de release mantiene `panic = "unwind"` para que `catch_unwind` tenga algo que atrapar.
- `Panel.qml` y `Costs.qml` indexan mapas dirigidos por datos con `Object.create(null)`, así que un bot o modelo llamado `__proto__` es solo un nombre.

## Binarios y releases

`bin/engine` se construye y firma con [`build-engine.yml`](.github/workflows/build-engine.yml) en cada push a `main` que cambie `engine/`, `mise.toml`, `scripts/build-engine.sh` o el propio workflow. Ejecuta primero la auditoría de dependencias y los tests — si alguno falla, no se firma nada — luego firma una atestación de procedencia de la compilación, versiona `bin/engine` con `SHA256SUMS` y la atestación como `provenance.sigstore.json`, y arranca [`verify-binaries.yml`](.github/workflows/verify-binaries.yml). [`release.yml`](.github/workflows/release.yml) publica esos archivos en un tag `v*` sin recompilar.

Para comprobar el binario tú mismo:

```sh
(cd bin && sha256sum -c SHA256SUMS)
gh attestation verify bin/engine --repo vk2r/mission --bundle bin/provenance.sigstore.json
```
