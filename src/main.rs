use std::io::{self, Write};
use std::ptr;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::NetworkManagement::Dhcp::{
    DhcpEnumSubnetClients, DhcpEnumSubnets, DhcpGetSubnetInfo, DhcpRpcFreeMemory,
    DHCP_CLIENT_INFO_ARRAY, DHCP_IP_ARRAY, DHCP_SUBNET_INFO,
};

fn ler_entrada(prompt: &str) -> String {
    print!("{}", prompt);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

fn ip_para_texto(ip: u32) -> String {
    let b = ip.to_be_bytes();
    format!("{}.{}.{}.{}", b[0], b[1], b[2], b[3])
}

// Função ajustada para ler corretamente a string descritiva do escopo via RPC
unsafe fn obter_nome_escopo(server: &HSTRING, subnet_ip: u32) -> String {
    let mut subnet_info_ptr: *mut DHCP_SUBNET_INFO = ptr::null_mut();
    
    let status = DhcpGetSubnetInfo(
        PCWSTR(server.as_ptr()),
        subnet_ip,
        &mut subnet_info_ptr,
    );

    if status == 0 && !subnet_info_ptr.is_null() {
        let info = &*subnet_info_ptr;
        
        // Tenta ler o nome do escopo (SubnetName) ou o comentário (SubnetComment)
        let nome = if !info.SubnetName.is_null() {
            info.SubnetName.to_string().unwrap_or_default()
        } else {
            String::new()
        };

        let comentario = if !info.SubnetComment.is_null() {
            info.SubnetComment.to_string().unwrap_or_default()
        } else {
            String::new()
        };

        DhcpRpcFreeMemory(subnet_info_ptr as *mut _);

        if !nome.is_empty() {
            if !comentario.is_empty() && comentario != nome {
                format!("{} ({})", nome, comentario)
            } else {
                nome
            }
        } else if !comentario.is_empty() {
            comentario
        } else {
            "Escopo Principal".to_string()
        }
    } else {
        format!("Escopo {}", ip_para_texto(subnet_ip))
    }
}

fn main() {
    println!("=========================================================");
    println!("         DHCP NMS - Gerenciador Portátil de Rede         ");
    println!("=========================================================");

    let server_input = ler_entrada("\nInsira o nome ou IP do servidor DHCP (ex: AMSNT): ");
    if server_input.is_empty() {
        println!("Nenhum servidor informado. Encerrando...");
        return;
    }

    let server_hstring = HSTRING::from(&server_input);

    loop {
        println!("\n[*] Consultando escopos em '{}'...", server_input);
        let mut resume_handle: u32 = 0;
        let mut enum_info: *mut DHCP_IP_ARRAY = ptr::null_mut();
        let mut elements_read: u32 = 0;
        let mut elements_total: u32 = 0;
        let mut escopos: Vec<u32> = Vec::new();

        unsafe {
            let status = DhcpEnumSubnets(
                PCWSTR(server_hstring.as_ptr()),
                &mut resume_handle,
                1024,
                &mut enum_info,
                &mut elements_read,
                &mut elements_total,
            );

            if status == 0 && !enum_info.is_null() {
                let array = &*enum_info;
                let subnets = std::slice::from_raw_parts(array.Elements, array.NumElements as usize);
                
                println!("\nEscopos IPv4 encontrados no servidor '{}':", server_input);
                for (i, &ip) in subnets.iter().enumerate() {
                    escopos.push(ip);
                    let nome_vlan = obter_nome_escopo(&server_hstring, ip);
                    println!("  [{}] {} -> {}", i + 1, ip_para_texto(ip), nome_vlan);
                }
                DhcpRpcFreeMemory(enum_info as *mut _);
            } else {
                eprintln!("\n[ERRO] Falha ao conectar em '{}'. Código Win32: {:#x}", server_input, status);
                break;
            }
        }

        if escopos.is_empty() {
            println!("Nenhum escopo encontrado.");
            break;
        }

        let escolha = ler_entrada("\nDigite o número do escopo para listar os IPs ativos (ou 0 para sair): ");
        if escolha == "0" || escolha.is_empty() {
            break;
        }

        if let Ok(idx) = escolha.parse::<usize>() {
            if idx >= 1 && idx <= escopos.len() {
                listar_clientes(&server_hstring, escopos[idx - 1]);
            } else {
                println!("Opção inválida.");
            }
        }
    }

    let _ = ler_entrada("\nPressione Enter para fechar...");
}

fn listar_clientes(server: &HSTRING, subnet_ip: u32) {
    println!("\n--- Concessões (Leases) no Escopo {} ---", ip_para_texto(subnet_ip));
    let mut resume_handle: u32 = 0;
    let mut client_info: *mut DHCP_CLIENT_INFO_ARRAY = ptr::null_mut();
    let mut clients_read: u32 = 0;
    let mut clients_total: u32 = 0;

    unsafe {
        let status = DhcpEnumSubnetClients(
            PCWSTR(server.as_ptr()),
            subnet_ip,
            &mut resume_handle,
            65536,
            &mut client_info,
            &mut clients_read,
            &mut clients_total,
        );

        if status == 0 && !client_info.is_null() {
            let array = &*client_info;
            let clients = std::slice::from_raw_parts(array.Clients, array.NumElements as usize);

            println!("{:<16} | {:<18} | {}", "ENDEREÇO IP", "MAC ADDRESS", "NOME DO HOST");
            println!("{}", "-".repeat(65));

            for &client_ptr in clients {
                if !client_ptr.is_null() {
                    let client = &*client_ptr;
                    let ip_str = ip_para_texto(client.ClientIpAddress);
                    
                    let mac_bytes = std::slice::from_raw_parts(
                        client.ClientHardwareAddress.Data,
                        client.ClientHardwareAddress.DataLength as usize,
                    );
                    let mac_str: Vec<String> = mac_bytes.iter().map(|b| format!("{:02X}", b)).collect();
                    let mac_formatted = mac_str.join("-");

                    let host_name = if !client.ClientName.is_null() {
                        client.ClientName.to_string().unwrap_or_else(|_| "Desconhecido".to_string())
                    } else {
                        "Sem Nome".to_string()
                    };

                    println!("{:<16} | {:<18} | {}", ip_str, mac_formatted, host_name);
                }
            }
            println!("\nTotal de clientes listados: {}", array.NumElements);
            DhcpRpcFreeMemory(client_info as *mut _);
        } else {
            eprintln!("Erro ao listar clientes (Código Win32: {:#x})", status);
        }
    }
}
