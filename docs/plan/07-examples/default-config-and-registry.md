# Example default and user configuration

**Examples only:** these keys illustrate desired behavior and are not a shipped schema. Official repository URL remains a placeholder until organization is chosen.

## Shipped default: `config/default.toml`

```toml
[app]
startup = "user-session"
locale = "system"

[registry]
default = "official"
auto_sync = true
cache_ttl = "24h"

[registries.official]
type = "git"
url = "https://github.com/YOUR_ORG/glint-library.git"
branch = "main"

[theme]
active_profile = "default"
auto_update = false
preview_before_apply = true

[packages]
allow_external_git = true
allow_local_archives = true
verify_integrity = true
allow_native_scripts = false

[security]
confirm_new_permissions = true
untrusted_network_default = "deny"

[shell]
use_default_fallback = true
safe_mode_after_crashes = 3
```

## User override (optional)

```toml
[theme]
active_profile = "work-setup"

[registries.friends]
type = "git"
url = "https://github.com/example/glint-collection.git"
branch = "main"

[packages]
allow_external_git = true
```

The merge policy should be explicit: keys overridden by user config, named registries merged by key, security defaults cannot be made weaker by a theme manifest. Admin/enterprise-managed policy (if ever offered) would be another higher-authority configuration layer.

## Registry versus theme manifest

`registry.json` indexes *many* packages and is primarily discovery metadata. `glint.json` describes *one* self-contained package. `config.toml` configures the *application*. A user profile describes *installed/active composition*. Keep these schemas distinct to avoid “registry entry = executable theme” confusion.

## Example custom source

```sh
glint registry add personal \
  https://github.com/example/personal-glint-themes.git

glint registry sync personal
glint theme install personal/monochrome
```

## Example direct distribution without GitHub

```sh
glint theme import ./dist/midnight.glint
glint theme use dev.example.midnight
```

No account and no network necessary for the two local commands. An imported archive that needs network to load its CSS/JS is invalid under the recommended self-contained format.
