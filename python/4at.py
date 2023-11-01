import socket
import threading
import time
import re

PORT = 6969
SAFE_MODE = True
MESSAGE_RATE = 1.0  
BAN_MESSAGE_LIMIT = 10 
BAN_LIMIT = 10 * 60.0
UNBAN_TIME = 10 * 60.0  
STRIKE_LIMIT = 10

def sensitive(message):
    if SAFE_MODE:
        return "[REDACTED]"
    return message

connected_clients = {}
banned_clients = {}

def broadcast(message, sender):
    for client_socket, client_info in connected_clients.items():
        if client_socket != sender:
            try:
                client_socket.send(message)
            except:
                # Remove the client if unable to send
                client_socket.close()
                del connected_clients[client_socket]

def is_valid_utf8(text):
    try:
        text.encode('utf-8').decode('utf-8')
        return True
    except UnicodeDecodeError:
        return False

def unban_clients():
    while True:
        current_time = time.time()
        for banned_client, unban_time in list(banned_clients.items()):
            if current_time >= unban_time:
                del banned_clients[banned_client]
                print(f"Client {sensitive(banned_client)} is unbanned.")
        time.sleep(1)

def handle_client(client_socket, client_address):
    print(f"Client {sensitive(client_address)} connected")
    connected_clients[client_socket] = {
        'address': client_address,
        'last_message_time': time.time(),
        'strike_count': 0,
        'message_count': 0
    }

    while True:
        try:
            message = client_socket.recv(1024)
            if not message:
                break
            decoded_message = message.decode('utf-8', 'ignore')
            print(f"Received: {decoded_message}")

            if is_valid_utf8(decoded_message):
                connected_clients[client_socket]['message_count'] += 1

                if connected_clients[client_socket]['message_count'] > BAN_MESSAGE_LIMIT:
                    if time.time() - connected_clients[client_socket]['last_message_time'] < 1 / MESSAGE_RATE:
                        ban_time = time.time() + BAN_LIMIT
                        print(f"Client {sensitive(client_address)} is banned for {BAN_LIMIT} seconds.")
                        banned_clients[client_address] = ban_time  # Ban the client
                        del connected_clients[client_socket]
                        client_socket.send(b"You are banned MF\n")
                        client_socket.close()
                        break
                    else:
                        connected_clients[client_socket]['message_count'] = 0
                connected_clients[client_socket]['last_message_time'] = time.time()
                
                broadcast(message, client_socket)
                connected_clients[client_socket]['strike_count'] = 0
            else:
                connected_clients[client_socket]['strike_count'] += 1

                if connected_clients[client_socket]['strike_count'] >= STRIKE_LIMIT:
                    ban_time = time.time() + BAN_LIMIT
                    print(f"Client {sensitive(client_address)} is banned for {BAN_LIMIT} seconds.")
                    banned_clients[client_address] = ban_time
                    del connected_clients[client_socket]
                    client_socket.send(b"You are banned MF\n")
                    client_socket.close()
                    break
        except Exception as e:
            print(f"Error: {e}")
            break

    print(f"Client {sensitive(client_address)} disconnected")
    if client_socket in connected_clients:
        del connected_clients[client_socket]
    client_socket.close()

def main():
    server_socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server_socket.bind(("", PORT))
    server_socket.listen(5)
    print(f"Listening to TCP connections on port {PORT} ...")

    # Start a thread to handle unbanning clients
    unbanning_thread = threading.Thread(target=unban_clients)
    unbanning_thread.daemon = True
    unbanning_thread.start()

    while True:
        client_socket, addr = server_socket.accept()
        client_handler = threading.Thread(target=handle_client, args=(client_socket, addr))
        client_handler.start()

if __name__ == "__main__":
    main()

