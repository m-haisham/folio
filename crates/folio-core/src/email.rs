use crate::{
    error::{FolioError, Result},
    types::{EmailConfig, FolioConfig},
};
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor, message::MultiPart,
    message::header::ContentType, transport::smtp::authentication::Credentials,
};

pub struct EmailMessage {
    pub to: String,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub body: String,
    pub attachment_path: Option<String>,
    pub attachment_name: Option<String>,
}

pub async fn send_email(config: &FolioConfig, msg: EmailMessage) -> Result<()> {
    let email_config = config
        .email
        .as_ref()
        .ok_or_else(|| FolioError::Other("Email not configured in folio.toml".to_string()))?;

    match email_config.provider.as_deref().unwrap_or("smtp") {
        "smtp" => send_smtp(email_config, config, msg).await,
        "sendgrid" => send_sendgrid(email_config, msg).await,
        "resend" => send_resend(email_config, msg).await,
        p => Err(FolioError::Other(format!("Unknown email provider: {}", p))),
    }
}

async fn send_smtp(email_cfg: &EmailConfig, config: &FolioConfig, msg: EmailMessage) -> Result<()> {
    let smtp = email_cfg
        .smtp
        .as_ref()
        .ok_or_else(|| FolioError::Other("SMTP config missing".to_string()))?;

    let password = std::env::var("FOLIO_SMTP_PASSWORD").unwrap_or_default();

    let from_addr = email_cfg.from.as_deref().unwrap_or(&config.me.email);
    let from_name = email_cfg.from_name.as_deref().unwrap_or(&config.me.name);

    let mut email_builder = Message::builder()
        .from(
            format!("{} <{}>", from_name, from_addr)
                .parse()
                .map_err(|e: lettre::address::AddressError| FolioError::Other(e.to_string()))?,
        )
        .to(msg
            .to
            .parse()
            .map_err(|e: lettre::address::AddressError| FolioError::Other(e.to_string()))?)
        .subject(&msg.subject);

    for cc in &msg.cc {
        email_builder = email_builder.cc(cc
            .parse()
            .map_err(|e: lettre::address::AddressError| FolioError::Other(e.to_string()))?);
    }

    let email = if let Some(attachment_path) = &msg.attachment_path {
        let attachment_bytes = std::fs::read(attachment_path)?;
        let attachment_name = msg.attachment_name.as_deref().unwrap_or("invoice.pdf");

        email_builder
            .multipart(
                MultiPart::mixed()
                    .singlepart(
                        lettre::message::SinglePart::builder()
                            .header(ContentType::TEXT_HTML)
                            .body(msg.body.clone()),
                    )
                    .singlepart(
                        lettre::message::SinglePart::builder()
                            .header(ContentType::parse("application/pdf").unwrap())
                            .header(lettre::message::header::ContentDisposition::attachment(
                                attachment_name,
                            ))
                            .body(attachment_bytes),
                    ),
            )
            .map_err(|e| FolioError::Other(e.to_string()))?
    } else {
        email_builder
            .singlepart(
                lettre::message::SinglePart::builder()
                    .header(ContentType::TEXT_HTML)
                    .body(msg.body.clone()),
            )
            .map_err(|e| FolioError::Other(e.to_string()))?
    };

    let creds = Credentials::new(smtp.username.clone(), password);
    let use_tls = smtp.tls.unwrap_or(true);
    let mailer = if use_tls {
        AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp.host)
            .map_err(|e| FolioError::Other(e.to_string()))?
            .credentials(creds)
            .port(smtp.port)
            .build()
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&smtp.host)
            .credentials(creds)
            .port(smtp.port)
            .build()
    };

    mailer
        .send(email)
        .await
        .map_err(|e| FolioError::Other(e.to_string()))?;

    Ok(())
}

async fn send_sendgrid(_cfg: &EmailConfig, _msg: EmailMessage) -> Result<()> {
    Err(FolioError::Other(
        "SendGrid not yet implemented".to_string(),
    ))
}

async fn send_resend(_cfg: &EmailConfig, _msg: EmailMessage) -> Result<()> {
    Err(FolioError::Other("Resend not yet implemented".to_string()))
}
