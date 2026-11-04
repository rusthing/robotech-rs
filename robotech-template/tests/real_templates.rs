//! 用四个真实模板验证新语法：websocket 载荷（6 个占位符）、position（14 个）、
//! SOS（21 个）、dbz 连接器异常（10 个）。
//!
//! 模板内容与 `dtw_config.rs` / `alarm_config.rs` / msg 种子迁移（转换后）保持一致。

use chrono::FixedOffset;
use robotech_template::{FilterContext, PriorityVariables, render_template};
use std::collections::HashMap;

fn ctx() -> FilterContext {
    FilterContext::new(
        FixedOffset::east_opt(8 * 3600).expect("合法时区"),
        1_735_689_700_000,
    )
}

fn render(template: &str, pairs: &[(&str, &str)]) -> String {
    let labels: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    let annotations: HashMap<String, String> = HashMap::new();
    let vars = PriorityVariables::new()
        .layer(&labels)
        .layer(&annotations);
    render_template(template, &vars, &ctx()).text
}

fn assert_json(text: &str, expected: impl FnOnce(&serde_json::Value)) {
    let parsed: serde_json::Value =
        serde_json::from_str(text).unwrap_or_else(|e| panic!("渲染结果不是合法 JSON: {e}\n{text}"));
    expected(&parsed);
}

const WEBSOCKET_PAYLOAD: &str = r#"{"eventCode":{{eventCode:json}},"title":{{title:json}},"content":{{content:json}},"ext":{{ext:json}},"firstReceiveMs":{{firstReceiveMs:default(null)}},"businessTriggerMs":{{businessTriggerMs:default(null)}}}"#;

#[test]
fn websocket_payload_all_variables() {
    let out = render(
        WEBSOCKET_PAYLOAD,
        &[
            ("eventCode", "dtw.locator.sos"),
            ("title", "定位卡 CARD-P-001 触发 SOS 报警"),
            ("content", "定位卡 CARD-P-001 触发 SOS 报警；位置：3F 层（经度 113.9，纬度 22.5）；绑定目标：张三；上报时间 2025-01-01 08:00:00"),
            ("ext", r#"{"alertId":"A-1","status":"active"}"#),
            ("firstReceiveMs", "1735689600000"),
            ("businessTriggerMs", "1735689600000"),
        ],
    );
    assert_eq!(
        out,
        r#"{"eventCode":"dtw.locator.sos","title":"定位卡 CARD-P-001 触发 SOS 报警","content":"定位卡 CARD-P-001 触发 SOS 报警；位置：3F 层（经度 113.9，纬度 22.5）；绑定目标：张三；上报时间 2025-01-01 08:00:00","ext":"{\"alertId\":\"A-1\",\"status\":\"active\"}","firstReceiveMs":1735689600000,"businessTriggerMs":1735689600000}"#
    );
    assert_json(&out, |v| {
        assert_eq!(v["eventCode"], "dtw.locator.sos");
        assert_eq!(v["firstReceiveMs"], 1735689600000_i64);
        assert!(v["ext"].is_string());
    });
}

#[test]
fn websocket_payload_missing_optional_fields_are_null() {
    let out = render(
        WEBSOCKET_PAYLOAD,
        &[
            ("eventCode", "dtw.locator.sos"),
            ("title", "定位卡 CARD-P-001 触发 SOS 报警"),
            ("content", "定位卡 CARD-P-001 触发 SOS 报警"),
        ],
    );
    assert_eq!(
        out,
        r#"{"eventCode":"dtw.locator.sos","title":"定位卡 CARD-P-001 触发 SOS 报警","content":"定位卡 CARD-P-001 触发 SOS 报警","ext":null,"firstReceiveMs":null,"businessTriggerMs":null}"#
    );
    assert_json(&out, |v| {
        assert!(v["ext"].is_null());
        assert!(v["firstReceiveMs"].is_null());
        assert!(v["businessTriggerMs"].is_null());
    });
}

const POSITION_TITLE: &str = "定位卡 {{locatorCode}} 位置已更新";
const POSITION_CONTENT: &str = "定位卡 {{locatorCode}} 位于 {{floor:default(未知)}} 层";
const POSITION_EXT: &str = r#"{"mode":{{mode}},"locatorCode":{{locatorCode:json}},"lon":{{lon}},"lat":{{lat}},"floor":{{floor:json}},"triggerTimestamp":{{triggerTimestamp}},"unityX":{{unityX}},"unityY":{{unityY}},"unityZ":{{unityZ}},"bindTargetType":{{bindTargetType}},"bindTargetId":{{bindTargetId}}}"#;

#[test]
fn position_templates_render() {
    let vars = [
        ("locatorCode", "L001"),
        ("floor", "3F"),
        ("mode", "1"),
        ("lon", "113.9"),
        ("lat", "22.5"),
        ("triggerTimestamp", "1735689600000"),
        ("unityX", "1.5"),
        ("unityY", "2.5"),
        ("unityZ", "0"),
        ("bindTargetType", "1"),
        ("bindTargetId", "1"),
    ];

    assert_eq!(render(POSITION_TITLE, &vars), "定位卡 L001 位置已更新");
    assert_eq!(render(POSITION_CONTENT, &vars), "定位卡 L001 位于 3F 层");

    let ext = render(POSITION_EXT, &vars);
    assert_eq!(
        ext,
        r#"{"mode":1,"locatorCode":"L001","lon":113.9,"lat":22.5,"floor":"3F","triggerTimestamp":1735689600000,"unityX":1.5,"unityY":2.5,"unityZ":0,"bindTargetType":1,"bindTargetId":1}"#
    );
    assert_json(&ext, |v| {
        assert_eq!(v["locatorCode"], "L001");
        assert_eq!(v["lon"], 113.9);
        assert_eq!(v["triggerTimestamp"], 1735689600000_i64);
    });
}

#[test]
fn position_content_floor_default() {
    assert_eq!(
        render(POSITION_CONTENT, &[("locatorCode", "L001")]),
        "定位卡 L001 位于 未知 层"
    );
}

const SOS_TITLE: &str = "定位卡 {{locatorCode}} 触发 SOS 报警";
const SOS_CONTENT: &str = "定位卡 {{locatorCode}} 触发 SOS 报警；位置：{{floor}} 层（经度 {{lon}}，纬度 {{lat}}）；绑定目标：{{bindTarget}}；上报时间 {{triggerTimestamp}}";
const SOS_EXT: &str = r#"{"alertId":{{alertId:json}},"eventCode":{{eventCode:json}},"locatorCode":{{locatorCode:json}},"lon":{{lon:json}},"lat":{{lat:json}},"floor":{{floor:json}},"unityX":{{unityX:json}},"unityY":{{unityY:json}},"unityZ":{{unityZ:json}},"bindTargetType":{{bindTargetType:json}},"bindTargetId":{{bindTargetId:json}},"triggerTimestamp":{{triggerTimestamp:json}},"positionTimestamp":{{positionTimestamp:json}}}"#;

#[test]
fn sos_templates_render() {
    let vars = [
        ("alertId", "A-1"),
        ("eventCode", "dtw.locator.sos"),
        ("locatorCode", "L001"),
        ("alertStatus", "active"),
        ("floor", "3F"),
        ("lon", "113.9"),
        ("lat", "22.5"),
        ("bindTarget", "张三"),
        ("bindTargetType", "1"),
        ("bindTargetId", "1"),
        ("triggerTimestamp", "2025-01-01 08:00:00"),
        ("positionTimestamp", "1735689600000"),
    ];

    assert_eq!(render(SOS_TITLE, &vars), "定位卡 L001 触发 SOS 报警");
    assert_eq!(
        render(SOS_CONTENT, &vars),
        "定位卡 L001 触发 SOS 报警；位置：3F 层（经度 113.9，纬度 22.5）；绑定目标：张三；上报时间 2025-01-01 08:00:00"
    );

    let ext = render(SOS_EXT, &vars);
    assert_eq!(
        ext,
        r#"{"alertId":"A-1","eventCode":"dtw.locator.sos","locatorCode":"L001","status":"active","lon":"113.9","lat":"22.5","floor":"3F","unityX":null,"unityY":null,"unityZ":null,"bindTargetType":"1","bindTargetId":"1","triggerTimestamp":"2025-01-01 08:00:00","positionTimestamp":"1735689600000"}"#
    );
    assert_json(&ext, |v| {
        assert_eq!(v["alertId"], "A-1");
        assert!(v["unityX"].is_null());
        assert_eq!(v["floor"], "3F");
    });
}

const DBZ_TITLE: &str = "Debezium {{directionName}}连接器<{{connectorName}}>异常";
const DBZ_CONTENT: &str = "{{directionName}}连接器<{{connectorName}}>运行状态异常：{{runStatusName}}（{{runStatusValue}}）；Debezium 连接器 {{connectorNameOnCluster}}；{{relation}}；所属 Kafka Connect<{{kafkaConnectName}}>：{{kafkaConnectUrl}}";

#[test]
fn dbz_templates_render() {
    let vars = [
        ("directionName", "上游"),
        ("connectorName", "c1"),
        ("runStatusName", "RUNNING"),
        ("runStatusValue", "1"),
        ("connectorNameOnCluster", "c1"),
        ("relation", "无上游连接器"),
        ("kafkaConnectName", "k1"),
        ("kafkaConnectUrl", "http://kafka-connect:8083"),
    ];

    assert_eq!(render(DBZ_TITLE, &vars), "Debezium 上游连接器<c1>异常");
    assert_eq!(
        render(DBZ_CONTENT, &vars),
        "上游连接器<c1>运行状态异常：RUNNING（1）；Debezium 连接器 c1；无上游连接器；所属 Kafka Connect<k1>：http://kafka-connect:8083"
    );
}

#[test]
fn websocket_payload_accepts_mixed_delimiters() {
    // 真实载荷里一半占位符改用 `${}`，渲染结果必须与全 `{{}}` 版本逐字一致
    let mixed = r#"{"eventCode":${eventCode:json},"title":{{title:json}},"content":${content:json},"ext":{{ext:json}},"firstReceiveMs":${firstReceiveMs:default(null)},"businessTriggerMs":{{businessTriggerMs:default(null)}}}"#;
    let vars = [
        ("eventCode", "dtw.locator.sos"),
        ("title", "定位卡 CARD-P-001 触发 SOS 报警"),
        ("content", "定位卡 CARD-P-001 触发 SOS 报警；位置：3F 层（经度 113.9，纬度 22.5）；绑定目标：张三；上报时间 2025-01-01 08:00:00"),
        ("ext", r#"{"alertId":"A-1","status":"active"}"#),
        ("firstReceiveMs", "1735689600000"),
        ("businessTriggerMs", "1735689600000"),
    ];

    let out = render(mixed, &vars);
    assert_eq!(out, render(WEBSOCKET_PAYLOAD, &vars));
    assert_json(&out, |v| {
        assert_eq!(v["eventCode"], "dtw.locator.sos");
        assert_eq!(v["firstReceiveMs"], 1735689600000_i64);
    });
}
