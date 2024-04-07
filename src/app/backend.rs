use chrono::{DateTime, Utc};
use egui::Color32;
use rand::rngs::ThreadRng;

use aes_gcm::aead::generic_array::GenericArray;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key,
};
use anyhow::{ensure, Result};
use argon2::{Config, Variant, Version};
use base64::engine::general_purpose;
use base64::Engine;
use rfd::FileDialog;
use rodio::{OutputStream, OutputStreamHandle, Sink};
use std::collections::BTreeMap;
use std::env;
use std::fmt::{Debug, Display};
use std::fs;
use std::io;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::string::FromUtf8Error;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{mpsc, Arc, Mutex};
use tonic::transport::{Channel, Endpoint};
use windows_sys::w;
use windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW;
use windows_sys::Win32::UI::WindowsAndMessaging::MB_ICONERROR;

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct TemplateApp {
    /*
        Font
    */
    ///fontbook
    pub filter: String,
    pub named_chars: BTreeMap<egui::FontFamily, BTreeMap<char, String>>,
    ///font
    pub font_size: f32,

    /*
    login page
    */
    ///the string entered to the username field on the login page
    pub login_username: String,

    #[serde(skip)]
    ///the string entered to the password field on the login page, dont save this one... obviously :)
    pub login_password: String,

    /*
        server main
    */
    ///SChecks whether server is already started TODO: FIX DUMB STUFF LIKE THIS, INSTEAD USE AN OPTION
    #[serde(skip)]
    pub server_has_started: bool,

    ///Public ip address, checked by pinging external website
    #[serde(skip)]
    pub public_ip: String,

    ///server settings
    pub server_req_password: bool,

    ///Server shutdown handler channel
    #[serde(skip)]
    pub server_shutdown_reciver: tokio::sync::mpsc::Receiver<()>,

    ///Server shutdown handler channel
    #[serde(skip)]
    pub server_shutdown_sender: tokio::sync::mpsc::Sender<()>,

    ///What is the server's password set to
    pub server_password: String,

    ///Which port is the server open on
    pub open_on_port: String,

    ///thread communication for server
    #[serde(skip)]
    pub srx: mpsc::Receiver<String>,
    #[serde(skip)]
    pub stx: mpsc::Sender<String>,

    ///child windows
    #[serde(skip)]
    pub settings_window: bool,

    ///thread communication for file requesting
    #[serde(skip)]
    pub frx: mpsc::Receiver<String>,
    #[serde(skip)]
    pub ftx: mpsc::Sender<String>,

    ///thread communication for image requesting
    #[serde(skip)]
    pub irx: mpsc::Receiver<String>,
    #[serde(skip)]
    pub itx: mpsc::Sender<String>,

    ///thread communication for audio recording
    #[serde(skip)]
    pub atx: Option<mpsc::Sender<bool>>,

    ///thread communication for audio ! SAVING !
    #[serde(skip)]
    pub audio_save_rx: mpsc::Receiver<(Option<Sink>, PlaybackCursor, usize, PathBuf)>,
    #[serde(skip)]
    pub audio_save_tx: mpsc::Sender<(Option<Sink>, PlaybackCursor, usize, PathBuf)>,

    /*
        main
    */
    pub main: Main,

    /*
        client main
    */
    pub client_ui: Client,

    #[serde(skip)]
    pub client_connection: ClientConnection,

    ///thread communication for client
    #[serde(skip)]
    pub rx: mpsc::Receiver<String>,
    #[serde(skip)]
    pub tx: mpsc::Sender<String>,

    ///data sync
    #[serde(skip)]
    pub drx: mpsc::Receiver<String>,
    #[serde(skip)]
    pub dtx: mpsc::Sender<String>,

    ///Server connection
    #[serde(skip)]
    pub connection_reciver: mpsc::Receiver<Option<ClientConnection>>,
    #[serde(skip)]
    pub connection_sender: mpsc::Sender<Option<ClientConnection>>,

    ///Server - client syncing thread
    #[serde(skip)]
    pub autosync_sender_thread: Option<()>,

    #[serde(skip)]
    pub autosync_output_reciver: Receiver<Option<String>>,
    #[serde(skip)]
    pub autosync_output_sender: Sender<Option<String>>,

    ///Server - client sync worker should run
    #[serde(skip)]
    pub autosync_should_run: Arc<AtomicBool>,

    #[serde(skip)]
    pub audio_file: Arc<Mutex<PathBuf>>,

    #[serde(skip)]
    pub opened_account: OpenedAccount,
}

impl Default for TemplateApp {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel::<String>();
        let (stx, srx) = mpsc::channel::<String>();
        let (dtx, drx) = mpsc::channel::<String>();
        let (ftx, frx) = mpsc::channel::<String>();
        let (itx, irx) = mpsc::channel::<String>();
        let (audio_save_tx, audio_save_rx) =
            mpsc::channel::<(Option<Sink>, PlaybackCursor, usize, PathBuf)>();

        let (connection_sender, connection_reciver) = mpsc::channel::<Option<ClientConnection>>();

        //Use the tokio sync crate for it to be async
        let (server_shutdown_sender, server_shutdown_reciver) = tokio::sync::mpsc::channel(1);

        let (autosync_output_sender, autosync_output_reciver) = mpsc::channel::<Option<String>>();

        Self {
            audio_file: Arc::new(Mutex::new(PathBuf::from(format!(
                "{}\\Matthias\\Client\\voice_record.wav",
                env!("APPDATA")
            )))),

            //fontbook
            filter: Default::default(),
            named_chars: Default::default(),

            //login page
            login_username: String::new(),
            login_password: String::new(),

            //server_main
            server_has_started: false,
            public_ip: String::new(),

            //server settings
            server_req_password: false,
            server_password: String::default(),
            open_on_port: String::default(),

            //thread communication for server
            srx,
            stx,

            //child windows
            settings_window: false,

            //thread communication for file requesting
            frx,
            ftx,

            //thread communication for image requesting
            irx,
            itx,
            
            //These default values will get overwritten when crating the new server, so we can pass in the reciver to the thread
            //Also, the shutdown reciver is unnecessary in this context because we never use it, I too lazy to delete a few lines instead of writing this whole paragraph >:D
            server_shutdown_reciver,
            server_shutdown_sender,

            //thread communication for audio recording
            atx: None,

            //thread communication for audio saving
            audio_save_rx,
            audio_save_tx,

            //main
            main: Main::default(),

            //client main
            client_ui: Client::default(),

            client_connection: ClientConnection::default(),

            //font
            font_size: 20.,

            //emoji button

            //thread communication for client
            rx,
            tx,

            //Server connection
            connection_sender,
            connection_reciver,

            //data sync
            drx,
            dtx,
            autosync_sender_thread: None,
            autosync_should_run: Arc::new(AtomicBool::new(true)),
            
            autosync_output_reciver,
            autosync_output_sender,

            opened_account: OpenedAccount::default(),
        }
    }
}

#[allow(dead_code)]
impl TemplateApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        if let Some(storage) = cc.storage {
            return eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        }

        Default::default()
    }
}

/*Children structs*/
///Children struct
/// Client Ui
#[derive(serde::Deserialize, serde::Serialize)]
pub struct Client {
    ///Search parameters set by user, to chose what to search for obviously
    pub search_parameter: SearchType,

    ///Check if search panel settings panel (xd) is open
    #[serde(skip)]
    pub search_settings_panel: bool,

    ///Search buffer
    #[serde(skip)]
    pub search_buffer: String,

    ///Check if search panel is open
    #[serde(skip)]
    pub search_mode: bool,

    ///Message highlighting function
    #[serde(skip)]
    pub message_highlight_color: Color32,

    ///emoji tray is hovered
    #[serde(skip)]
    pub emoji_tray_is_hovered: bool,

    ///audio playback
    #[serde(skip)]
    pub audio_playback: AudioPlayback,

    ///this doesnt really matter if we save or no so whatever, implements scrolling to message element
    #[serde(skip)]
    pub scroll_to_message: Option<ScrollToMessage>,

    ///index of the reply the user clicked on
    #[serde(skip)]
    pub scroll_to_message_index: Option<usize>,

    ///Selected port on sending
    pub send_on_port: String,

    ///Selected ip address (without port as seen above)
    pub send_on_address: String,

    ///This is used when the client entered a false password to connect with to the server
    #[serde(skip)]
    pub invalid_password: bool,

    ///This is set to on when an image is enlarged
    #[serde(skip)]
    pub image_overlay: bool,

    ///Scroll widget rect, text editor's rect
    pub scroll_widget_rect: egui::Rect,

    ///This decides how wide the text editor should be, ensure it doesnt overlap with "msg_action_tray" (the action buttons :) )
    pub text_widget_offset: f32,

    ///A vector of all the added files to the buffer, these are the PathBufs which get read, then their bytes get sent
    #[serde(skip)]
    pub files_to_send: Vec<PathBuf>,

    ///This checks if the text editor is open or not
    pub usr_msg_expanded: bool,

    ///This is the full address of the destionation a message is supposed to be sent to
    pub send_on_ip: String,

    ///self.send_on_ip encoded into base64, this is supposedly for ease of use, I dont know why its even here
    pub send_on_ip_base64_encoded: String,

    ///Does client have the password required checkbox ticked
    pub req_passw: bool,

    ///The password the user has entered for server auth
    pub client_password: String,

    ///This gem of a variable is used to contain animation's state
    pub animation_state: f32,

    ///This checks if a file is dragged above Matthias, so it knows when to display the cool animation 8)
    #[serde(skip)]
    pub drop_file_animation: bool,

    ////This indexes the user's selected messages for replying
    #[serde(skip)]
    pub replying_to: Option<usize>,

    ///Input (Múlt idő) user's message, this is what gets modified in the text editor
    #[serde(skip)]
    pub usr_msg: String,

    ///Incoming messages, this is the whole packet which get sent to all the clients, this cointains all the messages, and the info about them
    #[serde(skip)]
    pub incoming_msg: ServerMaster,

    /// Incoming messages len, its a mutex so it can sasfely sent between threads (for syncing)
    #[serde(skip)]
    pub incoming_msg_len: Arc<Mutex<usize>>,

    /// Last seen message's index, this will get sent 
    #[serde(skip)]
    pub last_seen_msg_index: Arc<Mutex<usize>>,

    ///emoji fasz
    pub random_emoji: String,
    pub emoji: Vec<String>,

    ///Random engine
    #[serde(skip)]
    pub rand_eng: ThreadRng,

    ///Used to decide whether the reactive emoji button should switch emojis (Like discords implementation)
    pub random_generated: bool,

    ///Log when the voice recording has been started so we know how long the recording is
    #[serde(skip)]
    pub voice_recording_start: Option<DateTime<Utc>>,

    ///When editing a message this buffer gets overwritten, and this gets sent which will overwrite the original message
    #[serde(skip)]
    pub text_edit_buffer: String,
}
impl Default for Client {
    fn default() -> Self {
        Self {
            search_parameter: SearchType::default(),
            search_settings_panel: false,
            search_buffer: String::new(),
            search_mode: false,

            message_highlight_color: Color32::WHITE,
            //audio playback
            audio_playback: AudioPlayback::default(),
            emoji_tray_is_hovered: false,
            scroll_widget_rect: egui::Rect::NAN,
            text_widget_offset: 0.0,
            scroll_to_message_index: None,
            scroll_to_message: None,
            send_on_port: String::new(),
            send_on_address: String::new(),
            invalid_password: false,
            image_overlay: false,
            files_to_send: Vec::new(),
            animation_state: 0.0,
            drop_file_animation: false,
            usr_msg_expanded: false,
            send_on_ip: String::new(),
            send_on_ip_base64_encoded: String::new(),
            req_passw: false,
            client_password: String::new(),
            emoji: vec![
                "😐", "😍", "😉", "😈", "😇", "😆", "😅", "😄", "😃", "😂", "😁", "😀",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>(),
            random_emoji: "🍑".into(),
            rand_eng: rand::thread_rng(),
            random_generated: false,

            //msg
            usr_msg: String::new(),
            replying_to: None,
            incoming_msg: ServerMaster::default(),

            voice_recording_start: None,
            text_edit_buffer: String::new(),
            incoming_msg_len: Arc::new(Mutex::new(0)),
            last_seen_msg_index: Arc::new(Mutex::new(0)),
        }
    }
}

///Main, Global stuff
#[derive(serde::Deserialize, serde::Serialize, Default)]
pub struct Main {
    ///Checks if windwos needs to be set up
    #[serde(skip)]
    pub setup: Option<()>,

    ///Checks if the emoji tray is on
    #[serde(skip)]
    pub emoji_mode: bool,

    ///Checks if bookmark mode is turned on
    #[serde(skip)]
    pub bookmark_mode: bool,

    ///Client mode main switch
    #[serde(skip)]
    pub client_mode: bool,

    ///IMPORTANT: Opened account's file pathbuf
    #[serde(skip)]
    pub opened_account_path: PathBuf,
}

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
///Opened account attributes, doesnt contain anything which might change at runtime
pub struct OpenedAccount {
    pub uuid: String,
    pub username: String,
    pub path: PathBuf,
}

impl OpenedAccount {
    pub fn new(uuid: String, username: String, path: PathBuf) -> Self {
        Self { uuid, username, path }
    }
}

///When the client is uploading a file, this packet gets sent
#[derive(Default, serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ClientFileUpload {
    pub extension: Option<String>,
    pub name: Option<String>,
    pub bytes: Vec<u8>,
}

///Normal message
#[derive(Default, serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ClientNormalMessage {
    pub message: String,
}

// Used for syncing or connecting & disconnecting
#[derive(Default, serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ClientSnycMessage {
    /// If its None its used for syncing, false: disconnecting, true: connecting
    /// If you have already registered the client with the server then the true value will be ignored
    pub sync_attribute: Option<bool>,

    ///This is used to tell the server how many messages it has to send, if its a None it will automaticly sync all messages
    /// This value is ignored if the `sync_attribute` field is Some(_)
    pub client_message_counter: Option<usize>,

    /// The index of the last seen message by the user, this is sent so we can display which was the last message the user has seen, if its None we ignore the value
    pub last_seen_message_index: Option<usize>,

    ///Contain password in the sync message, so we will send the password when authenticating
    pub password: String,
}

///This is used by the client for requesting file
#[derive(Default, serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ClientFileRequest {
    pub index: i32,
}

///This is used by the client for requesting images
#[derive(Default, serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ClientImageRequest {
    pub index: i32,
}

///Client requests audio file in server
#[derive(Default, serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ClientAudioRequest {
    pub index: i32,
}

///Reaction packet, defines which message its reacting to and with which char
#[derive(Default, serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ClientReaction {
    pub char: char,
    pub message_index: usize,
}

///Lets the client edit their *OWN* message, a client check is implemented TODO: please write a server check for this
#[derive(Default, serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ClientMessageEdit {
    ///The message which is edited
    pub index: usize,
    ///The new message
    pub new_message: Option<String>,
}

///These are the types of requests the client can ask
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub enum ClientFileRequestType {
    ///this is when you want to display an image and you have to make a request to the server file
    ClientImageRequest(ClientImageRequest),
    ClientFileRequest(ClientFileRequest),
    ClientAudioRequest(ClientAudioRequest),
}

///Client outgoing message types
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub enum ClientMessageType {
    ClientFileRequestType(ClientFileRequestType),

    ClientFileUpload(ClientFileUpload),

    ///Normal msg
    ClientNormalMessage(ClientNormalMessage),

    ///Used for syncing with client and server
    ClientSyncMessage(ClientSnycMessage),

    ClientReaction(ClientReaction),

    ClientMessageEdit(ClientMessageEdit)
}

///This is what gets to be sent out by the client
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ClientMessage {
    pub replying_to: Option<usize>,
    pub MessageType: ClientMessageType,
    pub Uuid: String,
    pub Author: String,
    pub MessageDate: String,
}

impl ClientMessage {
    ///struct into string, it makes sending information easier by putting it all in a string
    pub fn struct_into_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    ///this is used when sending a normal message
    pub fn construct_normal_msg(
        msg: &str,
        uuid: &str,
        author: &str,
        replying_to: Option<usize>,
    ) -> ClientMessage {
        ClientMessage {
            replying_to,
            MessageType: ClientMessageType::ClientNormalMessage(ClientNormalMessage {
                message: msg.trim().to_string(),
            }),
            //If the password is set as None (Meaning the user didnt enter any password) just send the message with an empty string
            Uuid: uuid.to_string(),
            Author: author.to_string(),
            MessageDate: { Utc::now().format("%Y.%m.%d. %H:%M").to_string() },
        }
    }

    ///this is used when you want to send a file, this contains name, bytes
    pub fn construct_file_msg(
        file_path: PathBuf,
        uuid: &str,
        author: &str,
        replying_to: Option<usize>,
    ) -> ClientMessage {
        ClientMessage {
            replying_to,
            //Dont execute me please :3 |
            //                          |
            //                          V
            MessageType: ClientMessageType::ClientFileUpload(ClientFileUpload {
                extension: Some(file_path.extension().unwrap().to_str().unwrap().to_string()),
                name: Some(
                    file_path
                        .file_prefix()
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_string(),
                ),
                bytes: std::fs::read(file_path).unwrap_or_default(),
            }),

            Uuid: uuid.to_string(),
            Author: author.to_string(),
            MessageDate: { Utc::now().format("%Y.%m.%d. %H:%M").to_string() },
        }
    }

    pub fn construct_reaction_msg(
        char: char,
        index: usize,
        author: &str,
        uuid: &str,
    ) -> ClientMessage {
        ClientMessage {
            replying_to: None,
            MessageType: ClientMessageType::ClientReaction(ClientReaction {
                char,
                message_index: index,
            }),
            Uuid: uuid.to_string(),
            Author: author.to_string(),
            MessageDate: { Utc::now().format("%Y.%m.%d. %H:%M").to_string() },
        }
    }

    /// this is used for constructing a sync msg aka sending an empty packet, so server can reply
    /// If its None its used for syncing, false: disconnecting, true: connecting
    pub fn construct_sync_msg(password: &str, author: &str, uuid: &str, client_message_counter: usize, last_seen_message_index: Option<usize>) -> ClientMessage {
        ClientMessage {
            replying_to: None,
            MessageType: ClientMessageType::ClientSyncMessage(ClientSnycMessage {
                sync_attribute: None,
                password: password.to_string(),
                //This value is not ignored in this context
                client_message_counter: Some(client_message_counter),
                last_seen_message_index,
            }),
            Uuid: uuid.to_string(),
            Author: author.to_string(),
            MessageDate: { Utc::now().format("%Y.%m.%d. %H:%M").to_string() },
        }
    }

    /// If its None its used for syncing, false: disconnecting, true: connecting
    pub fn construct_connection_msg(password: String, author: String, uuid: &str, last_seen_message_index: Option<usize>) -> ClientMessage {
        ClientMessage {
            replying_to: None,
            MessageType: ClientMessageType::ClientSyncMessage(ClientSnycMessage {
                sync_attribute: Some(true),
                password,
                //If its used for connecting / disconnecting this value is ignored
                client_message_counter: None,
                last_seen_message_index,
            }),
            Uuid: uuid.to_string(),
            Author: author,
            MessageDate: { Utc::now().format("%Y.%m.%d. %H:%M").to_string() },
        }
    }

    /// If its None its used for syncing, false: disconnecting, true: connecting
    /// Please note that its doesnt really matter what we pass in the author becuase the server identifies us based on our ip address
    pub fn construct_disconnection_msg(password: String, author: String, uuid: &str, last_seen_message_index: Option<usize>) -> ClientMessage {
        ClientMessage {
            replying_to: None,
            MessageType: ClientMessageType::ClientSyncMessage(ClientSnycMessage {
                sync_attribute: Some(false),
                password,
                //If its used for connecting / disconnecting this value is ignored
                client_message_counter: None,
                last_seen_message_index,
            }),
            Uuid: uuid.to_string(),
            Author: author,
            MessageDate: { Utc::now().format("%Y.%m.%d. %H:%M").to_string() },
        }
    }

    ///this is used for asking for a file
    pub fn construct_file_request_msg(
        index: i32,
        uuid: &str,
        author: String,
    ) -> ClientMessage {
        ClientMessage {
            replying_to: None,
            MessageType: ClientMessageType::ClientFileRequestType(
                ClientFileRequestType::ClientFileRequest(ClientFileRequest { index }),
            ),
            Uuid: uuid.to_string(),
            Author: author,
            MessageDate: { Utc::now().format("%Y.%m.%d. %H:%M").to_string() },
        }
    }

    ///this is used for asking for an image
    pub fn construct_image_request_msg(
        index: i32,
        uuid: &str,
        author: String,
    ) -> ClientMessage {
        ClientMessage {
            replying_to: None,
            MessageType: ClientMessageType::ClientFileRequestType(
                ClientFileRequestType::ClientImageRequest(ClientImageRequest { index }),
            ),
            Uuid: uuid.to_string(),
            Author: author,
            MessageDate: { Utc::now().format("%Y.%m.%d. %H:%M").to_string() },
        }
    }

    ///this is used for asking for an image
    pub fn construct_audio_request_msg(
        index: i32,
        uuid: &str,
        author: String,
    ) -> ClientMessage {
        ClientMessage {
            replying_to: None,
            MessageType: ClientMessageType::ClientFileRequestType(
                ClientFileRequestType::ClientAudioRequest(ClientAudioRequest { index }),
            ),
            Uuid: uuid.to_string(),
            Author: author,
            MessageDate: { Utc::now().format("%Y.%m.%d. %H:%M").to_string() },
        }
    }

    pub fn construct_client_message_edit(index: usize, new_message: Option<String>, uuid: &str, author: &str) -> ClientMessage {
        ClientMessage {
            replying_to: None,
            MessageType: ClientMessageType::ClientMessageEdit(ClientMessageEdit { index, new_message }),
            Uuid: uuid.to_string(),
            Author: author.to_string(),
            MessageDate: { Utc::now().format("%Y.%m.%d. %H:%M").to_string() },
        }
    }

    //this is used for SENDING IMAGES SO THE SERVER CAN DECIDE IF ITS A PICTURE
    //NOTICE: ALL THE AUDIO UPLOAD TYPES HAVE BEEN CONVERTED INTO ONE => "ClientFileUpload" this ensures that the client doesnt handle any backend stuff
}

///This manages all the settings and variables for maintaining a connection with the server (from client)
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default)]
pub struct ClientConnection {
    #[serde(skip)]
    pub client: Option<MessageClient<Channel>>,
    #[serde(skip)]
    pub client_secret: Vec<u8>,
    #[serde(skip)]
    pub state: ConnectionState,
}

impl ClientConnection {
    ///Ip arg to know where to connect, username so we can register with the sever, used to spawn a valid ClientConnection instance
    pub async fn connect(
        ip: String,
        author: String,
        password: Option<String>,
        uuid: &str,
    ) -> anyhow::Result<Self> {
        //Ping server to recive custom uuid, and to also get if server ip is valid
        let client = MessageClient::new(Endpoint::from_shared(ip.clone())?.connect_lazy());

        //This will later get modified
        let mut client_secret: Vec<u8> = Vec::new();

        let mut client_clone = client.clone();

        let client = match client_clone
            .message_main(tonic::Request::new(MessageRequest {
                //If its set to none then use a String::defult(), which is nothing
                message: ClientMessage::construct_connection_msg(
                    password.unwrap_or(String::from("")),
                    author,
                    uuid,
                    None,
                )
                .struct_into_string(),
            }))
            .await
        {
            /*We could return this, this is what the server is supposed to return, when a new user is connected */
            Ok(server_reply) => {
                let msg = server_reply.into_inner().message;

                ensure!(msg != "Invalid Password!", "Invalid password!");
                ensure!(msg != "Invalid Client!", "Outdated client or connection!");

                //This the key the server replied, and this is what well need to decrypt the messages, overwrite the client_secret variable
                client_secret = hex::decode(msg)?;

                Some(client_clone)
            }
            Err(err) => {
                display_error_message(err);

                None
            }
        };

        Ok(Self {
            client,
            client_secret,
            state: ConnectionState::Connected,
        })
    }

    ///Used to destroy a current ClientConnection instance does not matter if the instance is invalid
    pub async fn disconnect(&mut self, author: String, password: String, uuid: String) -> anyhow::Result<()> {
        //De-register with the server
        let client = self.client.as_mut().ok_or(anyhow::Error::msg(
            "Invalid ClientConnection instance (Client is None)",
        ))?;

        client
            .message_main(tonic::Request::new(MessageRequest {
                message: ClientMessage::construct_disconnection_msg(password, author, &uuid, None)
                    .struct_into_string(),
            }))
            .await?;

        Ok(())
    }
}

///Used to show state of the connection
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub enum ConnectionState {
    Connected,
    Disconnected,
    Connecting,
    Error,
}

impl Default for ConnectionState {
    fn default() -> Self {
        Self::Disconnected
    }
}

impl Debug for ConnectionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ConnectionState::Connected => "Connected",
            ConnectionState::Disconnected => "Disconnected",
            ConnectionState::Connecting => "Connecting",
            ConnectionState::Error => "Error",
        })
    }
}

/*
    Server. . .

    Are used to convert clinet sent messages into a server message, so it can be sent back;
    Therefor theyre smaller in size
*/

/*
        NOTICE:


    .... Upload : is always what the server sends back to the client (so the client knows what to ask about)

    .... Reply : is always what the server send to the client after the client asked.

*/

///This is what the server sends back (pushes to message vector), when reciving a file
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ServerFileUpload {
    pub file_name: String,
    pub index: i32,
}

///This is what the server sends back, when asked for a file (FIleRequest)
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ServerFileReply {
    pub bytes: Vec<u8>,
    pub file_name: PathBuf,
}

///This is what gets sent to a client basicly, and they have to ask for the file when the ui containin this gets rendered
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ServerImageUpload {
    pub index: i32,
}

///When client asks for the image based on the provided index, reply with the image bytes
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ServerImageReply {
    pub bytes: Vec<u8>,
    pub index: i32,
}

///This is what the server sends back (pushes to message vector), when reciving a normal message
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ServerNormalMessage {
    pub has_been_edited: bool,
    pub message: String,
}

///REFER TO -> ServerImageUpload; logic      ||      same thing but with audio files
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ServerAudioUpload {
    pub index: i32,
    pub file_name: String,
}

///When client asks for the image based on the provided index, reply with the audio bytes, which gets written so it can be opened by a readbuf
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ServerAudioReply {
    pub bytes: Vec<u8>,
    pub index: i32,
    pub file_name: String,
}

use strum::{EnumDiscriminants, EnumMessage};
use strum_macros::EnumString;

use super::client::messages::message_client::MessageClient;
use super::client::messages::MessageRequest;

///This is what server replies can be
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, EnumDiscriminants)]
#[strum_discriminants(derive(EnumString, EnumMessage))]
pub enum ServerMessageType {
    #[strum_discriminants(strum(message = "Upload"))]
    Upload(ServerFileUpload),
    #[strum_discriminants(strum(message = "Normal"))]
    Normal(ServerNormalMessage),

    ///Used to send and index to client so it knows which index to ask for VERY IMPORTANT!!!!!!!!!
    #[strum_discriminants(strum(message = "Image"))]
    Image(ServerImageUpload),
    #[strum_discriminants(strum(message = "Audio"))]
    Audio(ServerAudioUpload),
}

///This struct contains all the reactions of one message
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default)]
pub struct MessageReaction {
    pub message_reactions: Vec<Reaction>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Reaction {
    pub char: char,
    pub times: i64,
}

///This is one msg (packet), which gets bundled when sending ServerMain
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ServerOutput {
    /// Which message is this a reply to?
    /// The server stores all messages in a vector so this index shows which message its a reply to (if it is)
    pub replying_to: Option<usize>,
    /// Inner message which is *wrapped* in the ServerOutput 
    pub MessageType: ServerMessageType,
    /// The account's name who sent the message
    pub Author: String,
    /// The date when this message was sent
    pub MessageDate: String,
    /// The reactions added to the message
    pub reactions: MessageReaction,
    /// The user who sent this message's uuid
    pub uuid: String,
    /// EXPREIMENTAL: if the said message was seen by the user
    pub seen: bool,
}

impl ServerOutput {
    pub fn _struct_into_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn convert_type_to_servermsg(
        normal_msg: ClientMessage,
        index: i32,
        //Automaticly generated enum by strum
        upload_type: ServerMessageTypeDiscriminants,
        reactions: MessageReaction,
        uuid: String,
    ) -> ServerOutput {
        ServerOutput {
            replying_to: normal_msg.replying_to,
            MessageType:
                match normal_msg.MessageType {
                    ClientMessageType::ClientFileRequestType(_) => unimplemented!("Converting request packets isnt implemented, because they shouldnt be displayed by the client"),
                    ClientMessageType::ClientFileUpload(upload) => {
                        match upload_type {
                            ServerMessageTypeDiscriminants::Upload => {
                                ServerMessageType::Upload(
                                    ServerFileUpload {
                                        file_name: format!(
                                            "{}.{}",
                                            upload.name.unwrap_or_default(),
                                            upload.extension.unwrap_or_default()
                                        ),
                                        index,
                                    }
                                )
                            },
                            ServerMessageTypeDiscriminants::Normal => unreachable!(),
                            ServerMessageTypeDiscriminants::Image => {
                                ServerMessageType::Image(
                                    ServerImageUpload {
                                        index,
                                    }
                                )
                            },
                            ServerMessageTypeDiscriminants::Audio => {
                                ServerMessageType::Audio(
                                    ServerAudioUpload {
                                        index,
                                        file_name: format!(
                                            "{}.{}",
                                            upload.name.unwrap_or_default(),
                                            upload.extension.unwrap_or_default()
                                        ),
                                    }
                                )
                            },
                        }
                    },
                    ClientMessageType::ClientNormalMessage(message) => {
                        ServerMessageType::Normal(
                            ServerNormalMessage {
                                message: message.message,
                                //Set default value for incoming messages
                                has_been_edited: false,
                            }
                        )
                    },
                    ClientMessageType::ClientSyncMessage(_) => unimplemented!("Converting Sync packets isnt implemented, because they shouldnt be displayed to the client"),
                    ClientMessageType::ClientReaction(_) => unimplemented!("This enum has a side effect on one message's MessageReaction which is contained by a ServerMessage stored by the server. This must not be displayed by the client"),
                    ClientMessageType::ClientMessageEdit(_) => unimplemented!("This enum has a side effect on the server's vec which stores all messages, this must not be displayed by the client"),
                },
            Author: normal_msg.Author,
            MessageDate: normal_msg.MessageDate,
            reactions,
            uuid,
            seen: false,
        }
    }
}

///Used to put all the messages into 1 big pack (Bundling All the ServerOutput-s), Main packet, this gets to all the clients
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default)]
pub struct ServerMaster {
    ///All of the messages recived from the server
    pub struct_list: Vec<ServerOutput>,
}
impl ServerMaster {
    pub fn struct_into_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
    pub fn convert_vec_serverout_into_server_master(
        server_output_list: Vec<ServerOutput>,
    ) -> ServerMaster {
        ServerMaster {
            struct_list: server_output_list,
        }
    }
}

/*
 Client backend
*/

///Struct for global audio playback
pub struct AudioPlayback {
    ///Output stream
    pub stream: OutputStream,
    ///Output stream handle
    pub stream_handle: OutputStreamHandle,
    ///Audio sinks, these are the audios played
    pub sink_list: Vec<Option<Sink>>,
    ///Settings list for the sink_list (The audios being played)
    pub settings_list: Vec<AudioSettings>,
}

impl Default for AudioPlayback {
    fn default() -> Self {
        let (stream, stream_handle) = OutputStream::try_default().unwrap();
        Self {
            stream,
            stream_handle,
            sink_list: Vec::new(),
            settings_list: Vec::new(),
        }
    }
}

///This is used by the audio player, this is where you can set the speed and volume etc
pub struct AudioSettings {
    ///Volume for audio stream
    pub volume: f32,
    ///Speed for audio stream
    pub speed: f32,
    ///Reader cursor, for reading the sound file
    pub cursor: PlaybackCursor,

    ///This is only for ui usage
    pub is_loading: bool,

    ///Cursor position
    pub cursor_position: u64,

    ///Path to audio file
    pub path_to_audio: PathBuf,
}

///Initialize default values
impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            volume: 0.8,
            speed: 1.,
            cursor: PlaybackCursor::new(Vec::new()),
            is_loading: false,
            cursor_position: 0,
            path_to_audio: PathBuf::new(),
        }
    }
}

/*
Maunally create a struct which implements the following traits:
                                                            Read
                                                            Seek

So it can be used as a Arc<Mutex<()>>
*/
#[derive(Clone)]
pub struct PlaybackCursor {
    pub cursor: Arc<Mutex<io::Cursor<Vec<u8>>>>,
}

///Impl new so It can probe a file (in vec<u8> format)
impl PlaybackCursor {
    pub fn new(data: Vec<u8>) -> Self {
        let cursor = Arc::new(Mutex::new(io::Cursor::new(data)));
        PlaybackCursor { cursor }
    }
}

///Implement the Read trait
impl Read for PlaybackCursor {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut cursor = self.cursor.lock().unwrap();
        cursor.read(buf)
    }
}

///Implement the Seek trait
impl Seek for PlaybackCursor {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let mut cursor = self.cursor.lock().unwrap();
        cursor.seek(pos)
    }
}

pub struct ScrollToMessage {
    pub messages: Vec<egui::Response>,
    pub index: usize,
}

impl ScrollToMessage {
    pub fn new(messages: Vec<egui::Response>, index: usize) -> ScrollToMessage {
        ScrollToMessage { messages, index }
    }
}

/*
    Client
*/
///Used to decide what to search for (In the Message search bar), defined by the user
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Default)]
pub enum SearchType {
    Date,
    File,
    #[default]
    Message,
    Name,
    Reply,
}

///Implement display for SearchType so its easier to display
impl Display for SearchType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            SearchType::Name => "Name",
            SearchType::Message => "Message",
            SearchType::Date => "Date",
            SearchType::Reply => "Replies",
            SearchType::File => "File",
        })
    }
}

///Get ipv4 ip address from an external website
pub fn ipv4_get() -> Result<String, std::io::Error> {
    // Send an HTTP GET request to a service that returns your public IPv4 address
    let response = reqwest::blocking::get("https://ipv4.icanhazip.com/");
    // Check if the request was successful
    if response.is_ok() {
        let public_ipv4 = response.unwrap().text();

        Ok(public_ipv4.unwrap())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "Failed to fetch ip address",
        ))
    }
}

///Get ipv6 ip address from an external website
pub fn ipv6_get() -> Result<String, std::io::Error> {
    // Send an HTTP GET request to a service that returns your public IPv4 address
    let response = reqwest::blocking::get("https://ipv6.icanhazip.com/");
    // Check if the request was successful
    if response.is_ok() {
        let public_ipv4 = response.unwrap().text();

        Ok(public_ipv4.unwrap())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "Failed to fetch ip address",
        ))
    }
}

/// Account management
/// struct containing a new user's info, when serialized / deserialized it gets encrypted or decrypted
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default)]
pub struct UserInformation {
    /// username
    pub username: String,
    /// IMPORTANT: PASSWORD *IS* ENCRYPTED BY FUNCTIONS IMPLEMENTED BY THIS TYPE
    pub password: String,
    /// uuids are encrypted by the new function
    pub uuid: String,
    /// bookmarked ips are empty by default, IMPORTANT: THESE ARE *NOT* ENCRYPTED BY DEFAULT
    pub bookmarked_ips: Vec<String>,
}

impl UserInformation {
    ///All of the args are encrypted
    pub fn new(username: String, password: String, uuid: String) -> Self {
        Self {
            username,
            password: encrypt(password),
            uuid,
            bookmarked_ips: Vec::new(),
        }
    }

    /// Automaticly check hash with argon2 encrypted password (from the file)
    pub fn verify_password(&self, password: String) -> bool {
        pass_hash_match(password, self.password.clone())
    }

    /// This serializer function automaticly encrypts the struct with the *encrypt_aes256* fn to string
    pub fn serialize(&self) -> anyhow::Result<String> {
        Ok(encrypt_aes256(serde_json::to_string(&self)?, &[42; 32]).unwrap())
    }

    /// This deserializer function automaticly decrypts the string the *encrypt_aes256* fn to Self
    pub fn deserialize(serialized_struct: &str) -> anyhow::Result<Self> {
        Ok(serde_json::from_str::<Self>(
            &decrypt_aes256(serialized_struct, &[42; 32]).unwrap(),
        )?)
    }

    /// Write file to the specified path
    pub fn write_file(&self, user_path: PathBuf) -> anyhow::Result<()> {
        let serialized_self = self.serialize()?;

        let mut file = fs::File::create(user_path)?;

        file.write_all(serialized_self.as_bytes())?;

        file.flush()?;

        Ok(())
    }

    /// Add a bookmark entry which can be converted to a string
    pub fn add_bookmark_entry<T>(&mut self, item: T)
    where
        T: ToString,
    {
        self.bookmarked_ips.push(item.to_string());
    }

    /// Remove bookmark at index from the list, this can panic if the wrong index is passed in
    pub fn delete_bookmark_entry(&mut self, index: usize) {
        self.bookmarked_ips.remove(index);
    }
}

#[inline]
/// aes256 is decrypted by this function by a fixed key
pub fn decrypt_aes256(string_to_be_decrypted: &str, key: &[u8]) -> Result<String, FromUtf8Error> {
    let ciphertext = hex::decode(string_to_be_decrypted).unwrap();

    let key = Key::<Aes256Gcm>::from_slice(key);

    let cipher = Aes256Gcm::new(key);
    let nonce = GenericArray::from([69u8; 12]); // funny encryption key hehehe

    let plaintext = cipher.decrypt(&nonce, ciphertext.as_ref()).unwrap();
    String::from_utf8(plaintext)
}

/// aes256 is encrypted by this function by a fixed key
pub fn encrypt_aes256(string_to_be_encrypted: String, key: &[u8]) -> aes_gcm::aead::Result<String> {
    let key = Key::<Aes256Gcm>::from_slice(key);

    let cipher = Aes256Gcm::new(key);
    let nonce = GenericArray::from([69u8; 12]); // funny encryption key hehehe

    let ciphertext = cipher.encrypt(&nonce, string_to_be_encrypted.as_bytes().as_ref())?;
    let ciphertext = hex::encode(ciphertext);

    Ok(ciphertext)
}

#[inline]
/// Argon is used to encrypt this
fn encrypt(string_to_be_encrypted: String) -> String {
    let password = string_to_be_encrypted.as_bytes();
    let salt = b"c1eaa94ec38ab7aa16e9c41d029256d3e423f01defb0a2760b27117ad513ccd2";
    let config = Config {
        variant: Variant::Argon2i,
        version: Version::Version13,
        mem_cost: 65536,
        time_cost: 12,
        lanes: 5,
        secret: &[],
        ad: &[],
        hash_length: 64,
    };

    argon2::hash_encoded(password, salt, &config).unwrap()
}

#[inline]
fn pass_hash_match(to_be_verified: String, encoded: String) -> bool {
    argon2::verify_encoded(&encoded, to_be_verified.as_bytes()).unwrap()
}

///Check login
pub fn login(username: String, password: String) -> Result<PathBuf> {
    let app_data = env::var("APPDATA")?;

    let path = PathBuf::from(format!("{app_data}\\Matthias\\{username}.szch"));

    let file_contents: UserInformation = UserInformation::deserialize(&fs::read_to_string(&path)?)?;

    let user_check = username == file_contents.username;

    ensure!(user_check, "File corrupted at the username entry");

    let password_check = file_contents.verify_password(password);

    ensure!(password_check, "Invalid password");

    Ok(path)
}

///Register a new profile
pub fn register(username: String, passw: String) -> anyhow::Result<()> {
    if username.contains(' ') || username.contains('@') || username.contains(' ') {
        return Err(anyhow::Error::msg("Cant use special characters in name"));
    }

    let app_data = env::var("APPDATA")?;

    let user_path = PathBuf::from(format!("{app_data}\\Matthias\\{username}.szch"));

    //Check if user already exists
    if std::fs::metadata(&user_path).is_ok() {
        return Err(anyhow::Error::msg("User already exists"));
    }

    //Construct user info struct then write it to the appdata matthias folder
    UserInformation::new(
        username,
        passw,
        encrypt_aes256(generate_uuid(), &[42; 32]).unwrap(),
    )
    .write_file(user_path)?;

    Ok(())
}

///Write general file, this function takes in a custom path
pub fn write_file(file_response: ServerFileReply) -> Result<()> {
    let files = FileDialog::new()
        .set_title("Save to")
        .set_directory("/")
        .add_filter(
            file_response
                .file_name
                .extension()
                .unwrap()
                .to_string_lossy()
                .to_string(),
            &[file_response
                .file_name
                .extension()
                .unwrap()
                .to_string_lossy()
                .to_string()],
        )
        .save_file();

    if let Some(file) = files {
        fs::write(file, file_response.bytes)?;
    }

    Ok(())
}

///Write an image file to the appdata folder
#[inline]
pub fn write_image(file_response: &ServerImageReply, ip: String) -> Result<()> {
    //secondly create the folder labeled with the specified server ip

    let path = format!(
        "{}\\Matthias\\Client\\{}\\Images\\{}",
        env!("APPDATA"),
        general_purpose::URL_SAFE_NO_PAD.encode(ip),
        file_response.index
    );

    fs::write(path, &file_response.bytes)?;

    Ok(())
}

///Write an audio file to the appdata folder
#[inline]
pub fn write_audio(file_response: ServerAudioReply, ip: String) -> Result<()> {
    //secondly create the folder labeled with the specified server ip
    let path = format!(
        "{}\\Matthias\\Client\\{}\\Audios\\{}",
        env!("APPDATA"),
        general_purpose::URL_SAFE_NO_PAD.encode(ip),
        file_response.index
    );

    fs::write(path, file_response.bytes)?;

    Ok(())
}

///Generate uuid
pub fn generate_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

///Display Error message with a messagebox
pub fn display_error_message<T>(display: T)
where
    T: ToString + std::marker::Send + 'static,
{
    std::thread::spawn(move || unsafe {
        MessageBoxW(
            0,
            str::encode_utf16(display.to_string().as_str())
                .chain(std::iter::once(0))
                .collect::<Vec<_>>()
                .as_ptr(),
            w!("Error"),
            MB_ICONERROR,
        );
    });
}

// pub fn display_toast_notification() -> anyhow::Result<()> {
//     let toastmanager = winrt_toast::ToastManager::new("Test123");
//     let mut notif = Toast::new();
//     notif.text1("Title").text2("Body").text3("Footer");
//     toastmanager.show(&notif)?;
//     Ok(())
// }