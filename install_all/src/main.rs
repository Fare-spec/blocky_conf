use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

// Check if running as root
fn is_root() -> bool {
    match Command::new("id").arg("-u").output() {
        Ok(output) => String::from_utf8_lossy(&output.stdout).trim() == "0",
        Err(_) => false,
    }
}

// Execute a shell command with a spinner and status message
fn run_with_spinner(description: &str, command: &str) {
    let spinner = ProgressBar::new_spinner();
    spinner.set_message(description.to_string());
    spinner.enable_steady_tick(Duration::from_millis(100));
    let style = ProgressStyle::default_spinner()
        .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
        .template("{spinner} {msg}")
        .expect("Invalid progress bar template");
    spinner.set_style(style);

    let output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    match output {
        Ok(output) if output.status.success() => {
            spinner.finish_with_message(format!("{} {}", "[OK]".green(), description));
        }
        Ok(output) => {
            spinner.finish_and_clear();
            let code = output.status.code().unwrap_or(-1);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let suggestion = if stderr.contains("Temporary failure resolving") {
                "Suggestion: vérifiez votre connexion réseau et la configuration DNS (ex. /etc/resolv.conf), puis réessayez."
            } else if stderr.contains("E: Failed to fetch") {
                "Suggestion: exécutez 'apt-get update' ou ajoutez '--fix-missing' pour tenter de récupérer les paquets manquants."
            } else {
                ""
            };
            let suggestion_text = if suggestion.is_empty() {
                String::new()
            } else {
                format!("\n{}", suggestion)
            };
            eprintln!(
                "{} {} exited with code {}.\nCommand: {}\nError output: {}{}",
                "[ERREUR]".red(),
                description,
                code,
                command,
                stderr,
                suggestion_text
            );
            std::process::exit(1);
        }
        Err(err) => {
            spinner.finish_and_clear();
            eprintln!(
                "{} {} failed to execute.\nCommand: {}\nError: {}",
                "[ERREUR]".red(),
                description,
                command,
                err
            );
            std::process::exit(1);
        }
    }
}

fn main() {
    if !is_root() {
        eprintln!("{}", "Ce programme doit être lancé avec sudo ou en tant que root.".red());
        std::process::exit(1);
    }

    // Parse CLI arguments for DB user and password
    let args: Vec<String> = env::args().collect();
    let mut db_user_arg: Option<String> = None;
    let mut db_pass_arg: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-u" | "--user" => {
                if i + 1 < args.len() {
                    db_user_arg = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("{} Option -u nécessite un argument", "[ERREUR]".red());
                    std::process::exit(1);
                }
            }
            "-p" | "--password" => {
                if i + 1 < args.len() {
                    db_pass_arg = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("{} Option -p nécessite un argument", "[ERREUR]".red());
                    std::process::exit(1);
                }
            }
            "-h" | "--help" => {
                println!(
                    "Usage: sudo ./mon_script [-u user] [-p password]\nSi non fourni, l'entrée interactive sera demandée."
                );
                return;
            }
            _ => {
                i += 1;
            }
        }
    }

    // DB username
    let db_user = match db_user_arg {
        Some(u) => u,
        None => {
            print!("Nom d'utilisateur de la base de données: ");
            io::stdout().flush().unwrap();
            let mut input = String::new();
            io::stdin().read_line(&mut input).expect("Failed to read username");
            input.trim().to_string()
        }
    };

    // DB password
    let db_pass = match db_pass_arg {
        Some(p) => p,
        None => loop {
            print!("Mot de passe: ");
            io::stdout().flush().unwrap();
            let mut pass1 = String::new();
            io::stdin().read_line(&mut pass1).expect("Failed to read password");
            let pass1 = pass1.trim();

            print!("Confirmez le mot de passe: ");
            io::stdout().flush().unwrap();
            let mut pass2 = String::new();
            io::stdin().read_line(&mut pass2).expect("Failed to read password confirmation");
            let pass2 = pass2.trim();

            if pass1 == pass2 {
                break pass1.to_string();
            } else {
                eprintln!("{} Les mots de passe ne correspondent pas, veuillez réessayer.", "[ERREUR]".red());
            }
        },
    };

    // Display current IPv4 address
    let ip_output = Command::new("hostname")
        .arg("-I")
        .output()
        .expect("Failed to get server IP");
    let ip_str = String::from_utf8_lossy(&ip_output.stdout);
    let ip = ip_str.split_whitespace().find(|s| s.contains('.')).unwrap_or("").to_string();
    println!("Adresse IPv4 du serveur: {}", ip);

    

    println!("{}", "Lancement de l'installation complète de Blocky + Grafana + Prometheus".bold());

    // System updates
    run_with_spinner("Mise à jour des paquets", "apt update -y && apt upgrade -y");

    // Blocky installation
    run_with_spinner("Téléchargement de blocky", "curl -L https://github.com/0xERR0R/blocky/releases/download/v0.25/blocky_v0.25_Linux_x86_64.tar.gz | tar -xz");
    run_with_spinner("Installation de blocky", "mv blocky /usr/local/bin/ && chmod +x /usr/local/bin/blocky");
    run_with_spinner("Autorisation port 53 (setcap)", "setcap 'cap_net_bind_service=+ep' /usr/local/bin/blocky");

    // Create blocky user
    run_with_spinner("Création de l'utilisateur blocky", "useradd -r -d /etc/blocky -s /usr/sbin/nologin blocky || true");
    run_with_spinner("Création dossier /etc/blocky", "mkdir -p /etc/blocky && chown blocky:blocky /etc/blocky");

    // Configuration download
    run_with_spinner("Téléchargement configuration blocky", "cd /etc/blocky && wget https://raw.githubusercontent.com/Fare-spec/blocky_conf/refs/heads/main/config.yml");
    run_with_spinner("Téléchargement des blacklists", "cd /etc/blocky && curl -s --retry 3 ftp://ftp.ut-capitole.fr/pub/reseau/cache/squidguard_contrib/blacklists.tar.gz | tar -xzf -");
    // Update /etc/blocky/config.yml
    let config_path = "/etc/blocky/config.yml";
    let mut config_content = fs::read_to_string(config_path).expect("Failed to read config.yml");
    config_content = config_content
        .replace("{ip}", &ip)
        .replace("{user}", &db_user)
        .replace("{password}", &db_pass);
    fs::write(config_path, config_content).expect("Failed to write config.yml");
    println!("{} Les placeholders de config.yml ont été mis à jour.", "[OK]".green());

    // Prometheus
    run_with_spinner("Installation de Prometheus", "apt install -y prometheus");
    run_with_spinner("Redémarrage de Prometheus", "systemctl restart prometheus");

    // Grafana
    run_with_spinner("Préparation Grafana (apt sources)", "apt-get install -y apt-transport-https software-properties-common wget");
    run_with_spinner("Ajout de la clé Grafana", "mkdir -p /etc/apt/keyrings/ && wget -q -O - https://apt.grafana.com/gpg.key | gpg --dearmor | tee /etc/apt/keyrings/grafana.gpg > /dev/null");
    run_with_spinner("Ajout du dépôt Grafana", "echo \"deb [signed-by=/etc/apt/keyrings/grafana.gpg] https://apt.grafana.com stable main\" | tee -a /etc/apt/sources.list.d/grafana.list");
    run_with_spinner("Mise à jour APT", "apt-get update");
    run_with_spinner("Installation Grafana", "apt-get install grafana -y");
    run_with_spinner("Démarrage Grafana", "systemctl start grafana-server");
    run_with_spinner("Activation Grafana", "systemctl enable grafana-server");

    // MariaDB installation
    run_with_spinner("Installation MariaDB", "apt install -y mariadb-server");

    // SQL INIT with user inputs
    let sql = format!(
        "CREATE USER '{user}'@'localhost' IDENTIFIED BY '{pass}';\
        GRANT ALL PRIVILEGES ON *.* TO '{user}'@'localhost';\
        FLUSH PRIVILEGES;\
        CREATE DATABASE blocky;",
        user = db_user,
        pass = db_pass
    );
    println!("{}", "🛠 Initialisation de la base de données MariaDB...".cyan());
    let mut child = Command::new("mysql")
        .stdin(Stdio::piped())
        .spawn()
        .expect("Failed to execute mysql");

    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(sql.as_bytes()).expect("Failed to write SQL");
    }
    let status = child.wait().expect("Failed to wait for mysql");
    if status.success() {
        println!("{}", "[OK] Initialisation MariaDB terminée".green());
    } else {
        eprintln!("{}", "[ERREUR] Initialisation MariaDB".red());
        std::process::exit(1);
    }
    // Blocked_page creation
    run_with_spinner("Recupération de la page", "cd /usr/local/bin && wget https://github.com/Fare-spec/blocky_conf/releases/download/page/blocked_page_server && mkdir ~/blocked_page && cd ~/blocked_page && mkdir urls && wget https://raw.githubusercontent.com/Fare-spec/blocky_conf/refs/heads/main/blocked_page/forbidden.html");
    run_with_spinner("Création d'un service blocked_page", "cd /etc/systemd/system/ && wget https://raw.githubusercontent.com/Fare-spec/blocky_conf/refs/heads/main/blocked_page.service");
    run_with_spinner("Accorder le port 80", "setcap 'cap_net_bind_service=+ep' /usr/local/bin/blocked_page_server");
    run_with_spinner("Redemmarage des services", "systemctl daemon-reload && systemctl enable blocked_page.service && systemctl start blocked_page.service");

    // Service
    run_with_spinner("Installation du service systemd blocky", "cd /etc/systemd/system/ && wget https://raw.githubusercontent.com/Fare-spec/blocky_conf/refs/heads/main/blocky.service");
    run_with_spinner("Rechargement systemd", "systemctl daemon-reload");
    run_with_spinner("Activation et démarrage de blocky", "systemctl enable --now blocky");

    // DNS conflict resolution
    run_with_spinner("Arrêt de systemd-resolved", "systemctl stop systemd-resolved");
    run_with_spinner("Désactivation de systemd-resolved", "systemctl disable systemd-resolved");
    run_with_spinner("Redémarrage de blocky", "systemctl restart blocky");


    println!("\n{}",
        format!("Installation terminée avec succès. Ouvre http://{}:3000 pour configurer Grafana.",ip).bold().green()
    );
}

