pragma ComponentBehavior: Bound
import QtQuick
import qs.Commons
import qs.Commons as Commons
import qs.Ui
import "../lang/index.js" as I18n
import "../components"

Column {
  id: page
  required property var host

  width: parent ? parent.width : 0
  spacing: Style.space(16)

  component SettingsTitle: Text {
    id: settingsTitle
    property string title: ""
    textFormat: Text.PlainText
    text: settingsTitle.title
    color: page.host.foreground
    font.family: page.host.fontFamily
    font.pixelSize: Style.font.caption
    font.bold: true
    font.letterSpacing: 1
  }

  component SettingCheck: Item {
    id: settingCheck
    property string label: ""
    property color tint: page.host.foreground
    property bool checked: true
    readonly property bool hot: checkArea.containsMouse
    signal toggled()

    implicitWidth: checkSquare.width + Style.space(6) + checkLabel.implicitWidth
    implicitHeight: Math.max(checkSquare.height, checkLabel.implicitHeight)
    opacity: settingCheck.checked ? 1 : 0.4
    Behavior on opacity { NumberAnimation { duration: 160 } }

    Rectangle {
      id: checkSquare
      anchors.verticalCenter: parent.verticalCenter
      width: Style.space(14)
      height: width
      radius: Style.space(3)
      color: settingCheck.checked ? settingCheck.tint : "transparent"
      border.width: 1
      border.color: settingCheck.checked || settingCheck.hot ? settingCheck.tint : page.host.dim

      Text {
        anchors.centerIn: parent
        visible: settingCheck.checked
        textFormat: Text.PlainText
        text: "\udb80\udd2c"
        color: Commons.Color.popups.background
        font.family: page.host.fontFamily
        font.pixelSize: Style.font.caption
      }
    }

    Text {
      id: checkLabel
      anchors.verticalCenter: parent.verticalCenter
      anchors.left: checkSquare.right
      anchors.leftMargin: Style.space(6)
      textFormat: Text.PlainText
      text: settingCheck.label
      color: settingCheck.tint
      font.family: page.host.fontFamily
      font.pixelSize: Style.font.caption
      font.bold: true
      font.letterSpacing: 1
    }

    MouseArea {
      id: checkArea
      anchors.fill: parent
      hoverEnabled: true
      cursorShape: Qt.PointingHandCursor
      onClicked: settingCheck.toggled()
    }
  }

  Column {
    width: parent.width
    spacing: Style.space(10)

    Row {
      spacing: Style.space(12)

      Button {
        anchors.verticalCenter: parent.verticalCenter
        iconText: "\udb80\udc4d"
        bordered: false
        foreground: page.host.foreground
        fontFamily: page.host.fontFamily
        tooltipText: page.host.t("backTip")
        onClicked: page.host.goTo("home")
      }

      Text {
        anchors.verticalCenter: parent.verticalCenter
        textFormat: Text.PlainText
        text: page.host.t("settings")
        color: page.host.foreground
        font.family: page.host.fontFamily
        font.pixelSize: Style.font.title
        font.bold: true
      }
    }

    PanelSeparator { width: parent.width; foreground: page.host.foreground }
  }

  Column {
    width: parent.width
    spacing: Style.space(10)

    SettingsTitle { title: page.host.t("sectionWidget").toUpperCase() }

    Item {
      width: parent.width
      height: languageLabel.implicitHeight + Style.space(6)

      Text {
        id: languageLabel
        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
        width: Style.space(90)
        textFormat: Text.PlainText
        text: page.host.t("language")
        color: page.host.dim
        font.family: page.host.fontFamily
        font.pixelSize: Style.font.caption
      }

      CompactDropdown {
        id: languagePicker
        anchors.left: languageLabel.right
        anchors.leftMargin: Style.space(10)
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        fontSize: Style.font.caption
        rowHeight: languageLabel.implicitHeight + Style.space(6)
        popupRowHeight: languageLabel.implicitHeight + Style.space(8)
        showLabel: false
        fontFamily: page.host.fontFamily
        options: [{ value: "auto", label: page.host.t("systemLanguage", [I18n.languageName(page.host.systemLang)]) }]
          .concat(I18n.LANGUAGES.map(function(l) { return { value: l.code, label: l.name } }))
        value: page.host.languageSetting
        onChanged: function(picked) {
          page.host.save("language", picked, "auto")
          languagePicker.value = Qt.binding(function() { return page.host.languageSetting })
        }
      }
    }
  }

  Column {
    width: parent.width
    spacing: Style.space(10)

    SettingsTitle { title: page.host.t("sectionCosts").toUpperCase() }

    Item {
      width: parent.width
      height: periodLabel.implicitHeight + Style.space(6)

      Text {
        id: periodLabel
        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
        width: Style.space(90)
        textFormat: Text.PlainText
        text: page.host.t("costPeriod")
        color: page.host.dim
        font.family: page.host.fontFamily
        font.pixelSize: Style.font.caption
      }

      RangePicker {
        id: periodPicker
        host: page.host
        anchors.left: periodLabel.right
        anchors.leftMargin: Style.space(10)
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        fontSize: Style.font.caption
        rowHeight: periodLabel.implicitHeight + Style.space(6)
        popupRowHeight: periodLabel.implicitHeight + Style.space(8)
      }
    }
  }

  Column {
    width: parent.width
    spacing: Style.space(10)

    SettingsTitle { title: page.host.t("sectionOther").toUpperCase() }

    SettingCheck {
      label: page.host.t("sectionNotifications")
      checked: page.host.notifyOn
      onToggled: page.host.save("notify", !page.host.notifyOn, true)
    }

    SettingCheck {
      label: page.host.t("showCostFooter")
      checked: page.host.showCost
      onToggled: page.host.save("showCost", !page.host.showCost, false)
    }

    SettingCheck {
      label: page.host.t("pixelAvatars")
      checked: page.host.pixelAvatars
      onToggled: page.host.save("pixelAvatars", !page.host.pixelAvatars, false)
    }

    SettingCheck {
      label: page.host.t("autoLaunch")
      visible: page.host.installed
      checked: page.host.autoLaunch
      onToggled: page.host.save("autoLaunch", !page.host.autoLaunch, false)
    }
  }
}
