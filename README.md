# MCSC 
Controller for minecraft servers written in rust, it
allows for starting, stopping, taking backups, downloading 
backups and running remote commands _without_ ssh(ing) into the server.
This should make it safe for you to allow
freinds ect to control the server without compromizing it's security as the
software can only perform a limmited  number of safe actions.
It is set
up with saftey in mind and will only allow one action to be run at a time to
avoid any pottential data corruption issues.  
This is **not** an installer and all
minecarft setup is left up-to the user.  
Any contributions wellcome.


# Building the project: 
## Nix
Nix can be [installed](https://nix.dev/tutorials/install-nix) on any machine

Just build
```fish
nix build
```

Build and run
```fish
nix run

```
## Non nix enabled systems
make sure you have cargo and protobuf installed:   
From the package manager 
- (void/xpbs): 
```fish
sudo xbps-install cargo protobuf 
```
- (debian/apt):
```fish
sudo apt install cargo protobuf
``` 
Clone the repo and install with cargo:   
```fish
git clone https://github.com/ElSargo/mcsc   
cd mcsc   
cargo install --path ./ 
```
The server and client need to read their respective config files so make sure to run them in the same directory and to set them up properly


# Planned Features
- [ ] permissions
- [ ] java version manager  
- [ ] minecraft version manager
- [ ] mod retriver using modrinth
- [ ] async api
- [ ] gui
- [ ] web ui
  
