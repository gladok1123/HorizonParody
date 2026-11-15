Horizon без лаунчера и без входа в аккаунт Microsoft: распакуйте архив, запустите `play.bat` (Windows) или `./play.sh` (Linux), выберите имя игрока.

**Что нужно для первого запуска**

1. Minecraft: Java Edition 26.3, скачанный вашим лаунчером и запущенный в нём хотя бы один раз: официальным, PineconeMC (ElyPrismLauncher) или Prism Launcher. Скрипт запуска сам находит их папки; портативную установку укажите в `minecraft-dir.txt`. В архиве нет файлов Minecraft: игра готовит текстуры, звуки и данные из вашей копии и сверяет каждый файл с контрольными суммами оригинала.
2. Java 25 JDK, например [Eclipse Temurin 25](https://adoptium.net/) (именно JDK: при первом запуске компилируются экспортёры; Java, которую лаунчер скачивает для игры, не подойдёт).

Первый запуск готовит ресурсы несколько минут, следующие сразу открывают игру. Подробности — в `PLAY.ru.txt` внутри архива.

**Что работает без аккаунта:** одиночная игра, локальная сеть, серверы с `online-mode=false`. Серверы с проверкой аккаунтов не пускают.

| Файл | Система |
|---|---|
| `horizon-offline-windows-x64.zip` | Windows 10/11, x64 |
| `horizon-offline-linux-x86_64.tar.gz` | Linux x86_64, glibc 2.35+ |

---

Horizon without the launcher and without a Microsoft account: unpack, run `play.bat` (Windows) or `./play.sh` (Linux), choose a player name. The first run needs your own Minecraft: Java Edition 26.3 (downloaded by the official launcher, PineconeMC or Prism Launcher and started there once) and a Java 25 JDK; `PLAY.txt` in the archive has the details. Single player, LAN and `online-mode=false` servers work; servers that check accounts refuse the name.

Horizon is an unofficial fan project, not affiliated with Mojang Studios or Microsoft.
