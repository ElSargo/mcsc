# mcsc
Controller for minecraft servers written in rust.
Allows for starting, stopping, taking backups, and downloading backups and running remote commands without ssh(ing) into the server.
This should make it safe for you to allow freinds ect to control the server without compromizing it's security as the software can only perform a limmited number of safe actions.
It is set up with saftey in mind and will only allow one action to be run at a time to avoid any pottential data corruption issues.
This is not an installer and all minecarft setup is left up-to the user. 
Any contributions wellcome
