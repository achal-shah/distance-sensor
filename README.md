# Measuring water level in a sump pump
Experimenting with a laser based time of flight sensor to measure the water level in a sump pump bucket.

## Why?
Three reasons - one is just the curiosity of seeing what is going on.  Second, to work with micro-controllers - getting closer to the metal.  Third, to learn the Rust
language.  Luckily the ESP-32 microprocessor I decided to use has a Rust libraries.

### TO-DO
1. The wifi provisioning library - [esp-wifi-provisioning](https://github.com/Dwarf1er/esp-wifi-provisioning.git) was cloned locally and the versions of esp-idf-svc and esp-idf-hal
   are updated to versions 0.52.1 and 0.46.2 respectively in a private local repository.  This will be reversed once the actual repo is updated with new versions.
2. Minor change in the file nvs.rs to add mut keyword to a couple of lines.  The compiler will tell you which ones.
